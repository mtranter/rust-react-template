use super::events;
use super::{
    ArchiveNote, DeferredJobs, EventPublisher, Expected, ListQuery, Listed, NoteRepository,
    NoteSortKey, Transactions, Tx, Version, Versioned,
};
use crate::domain::{
    AuthorId, CreateNote, EditNote, ItemId, Note, NoteAction, NoteError, NoteEvent, NoteId,
};
use app_authz::{Authorizer, Plan, Principal};
use app_domain::Clock;
use app_error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use users_api::UserDirectory;

/// Holds trait objects rather than being generic, so the service is one concrete
/// type that can be named and shared; one dynamic call is negligible beside the I/O.
#[derive(Clone)]
pub struct NoteService {
    repo: Arc<dyn NoteRepository>,
    users: Arc<dyn UserDirectory>,
    publisher: Arc<dyn EventPublisher>,
    deferred: Arc<dyn DeferredJobs>,
    transactions: Arc<dyn Transactions>,
    clock: Arc<dyn Clock>,
    authorizer: Arc<Authorizer>,
}

impl NoteService {
    pub fn new(
        repo: Arc<dyn NoteRepository>,
        users: Arc<dyn UserDirectory>,
        publisher: Arc<dyn EventPublisher>,
        deferred: Arc<dyn DeferredJobs>,
        transactions: Arc<dyn Transactions>,
        clock: Arc<dyn Clock>,
        authorizer: Arc<Authorizer>,
    ) -> Self {
        Self {
            repo,
            users,
            publisher,
            deferred,
            transactions,
            clock,
            authorizer,
        }
    }

    /// Asked first in every use case, before anything is loaded or begun. A
    /// `Where` plan (a scope, such as `author`) is checked against the note
    /// once it is loaded ([`load_for`](Self::load_for)).
    fn authorize(&self, principal: &Principal, action: NoteAction) -> AppResult<Plan> {
        self.authorizer.authorize::<Note>(principal, action)
    }

    /// The caller is the author. The author check and the save aren't one
    /// transaction: they're different modules. An author deleted in between is
    /// caught by the `UserDeleted` reaction only if it runs after this commits.
    pub async fn create(
        &self,
        principal: &Principal,
        title: String,
        body: String,
        archive_at: Option<DateTime<Utc>>,
    ) -> AppResult<Versioned<Note>> {
        self.authorize(principal, NoteAction::Create)?;
        let not_a_user = || AppError::validation("only a registered user can write a note");
        let author: AuthorId = principal
            .id()
            .and_then(|id| id.parse().ok())
            .ok_or_else(not_a_user)?;
        let cmd = CreateNote {
            author_id: author,
            title,
            body,
            archive_at,
        };
        let (note, event) = Note::create(NoteId::new(), cmd, self.clock.now())?;
        if !self.users.exists(author.as_uuid()).await? {
            return Err(not_a_user());
        }
        let mut tx = self.transactions.begin().await?;
        let version = self.repo.save(&mut tx, &note, Expected::New).await?;
        self.publisher
            .publish(&mut tx, events::pending(&note, version, event))
            .await?;
        self.schedule_archive(&mut tx, &note, None).await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: note,
            version,
        })
    }

    /// `expected` is the version the caller last read (from `If-Match`).
    pub async fn edit(
        &self,
        principal: &Principal,
        cmd: EditNote,
        expected: Version,
    ) -> AppResult<Versioned<Note>> {
        let plan = self.authorize(principal, NoteAction::Edit)?;
        self.change(principal, &plan, cmd.note_id, expected, |note| {
            note.edit(cmd)
        })
        .await
    }

    /// An item is a child of its note: the note's version guards it, and its
    /// change is the note's, announced on the note's topic.
    pub async fn add_item(
        &self,
        principal: &Principal,
        note: NoteId,
        text: &str,
        expected: Version,
    ) -> AppResult<Versioned<Note>> {
        let plan = self.authorize(principal, NoteAction::AddItem)?;
        let id = ItemId::new();
        self.change(principal, &plan, note, expected, |note| {
            note.add_item(id, text)
        })
        .await
    }

    pub async fn remove_item(
        &self,
        principal: &Principal,
        note: NoteId,
        item: ItemId,
        expected: Version,
    ) -> AppResult<Versioned<Note>> {
        let plan = self.authorize(principal, NoteAction::RemoveItem)?;
        self.change(principal, &plan, note, expected, |note| {
            note.remove_item(item)
        })
        .await
    }

    async fn change(
        &self,
        principal: &Principal,
        plan: &Plan,
        id: NoteId,
        expected: Version,
        change: impl FnOnce(&mut Note) -> Result<NoteEvent, NoteError>,
    ) -> AppResult<Versioned<Note>> {
        // Load first so a missing note is a not-found rather than a stale-version
        // conflict; the version check itself is atomic in the repository's update.
        let mut note = self.load_for(principal, plan, id).await?.entity;
        let archive_at = note.archive_at();
        let event = change(&mut note)?;
        let mut tx = self.transactions.begin().await?;
        let version = self
            .repo
            .save(&mut tx, &note, Expected::Version(expected))
            .await?;
        self.publisher
            .publish(&mut tx, events::pending(&note, version, event))
            .await?;
        self.schedule_archive(&mut tx, &note, archive_at).await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: note,
            version,
        })
    }

    /// Removes the note and announces it, as it was last, in the same transaction.
    pub async fn delete(
        &self,
        principal: &Principal,
        id: NoteId,
        expected: Version,
    ) -> AppResult<()> {
        let plan = self.authorize(principal, NoteAction::Delete)?;
        let current = self.load_for(principal, &plan, id).await?;
        // Checked here as well as by the delete, so the state announced is the
        // version deleted; the event announces the version after it, so it's the
        // newest of its key.
        if current.version != expected {
            return Err(AppError::precondition_failed(
                "the note was modified by someone else",
            ));
        }
        let event = current.entity.delete()?;
        let mut tx = self.transactions.begin().await?;
        self.repo.delete(&mut tx, id, expected).await?;
        self.publisher
            .publish(
                &mut tx,
                events::pending(&current.entity, expected.next(), event),
            )
            .await?;
        self.deferred
            .cancel::<ArchiveNote>(&mut tx, ArchiveNote::key(id))
            .await?;
        tx.commit().await
    }

    /// Defers the note's archive to its `archive_at` in `tx`, replacing the
    /// one deferred before, or cancels it once cleared; nothing when it
    /// hasn't changed from `before`.
    async fn schedule_archive(
        &self,
        tx: &mut Tx,
        note: &Note,
        before: Option<DateTime<Utc>>,
    ) -> AppResult<()> {
        if note.archive_at() == before {
            return Ok(());
        }
        let key = ArchiveNote::key(note.id());
        match note.archive_at() {
            Some(at) => {
                let job = ArchiveNote {
                    note_id: note.id().as_uuid(),
                };
                self.deferred.defer(tx, key, at, &job).await
            }
            None => self.deferred.cancel::<ArchiveNote>(tx, key).await,
        }
    }

    /// Archives the note once its `archive_at` has come, in `tx`. A note
    /// already archived, gone, or no longer due (its time cleared or moved
    /// later) is left as it is, so a stale job does nothing.
    pub async fn archive_when_due(
        &self,
        principal: &Principal,
        tx: &mut Tx,
        id: NoteId,
    ) -> AppResult<()> {
        let plan = self.authorize(principal, NoteAction::Archive)?;
        let now = self.clock.now();
        let Some(Versioned {
            entity: mut note,
            version,
        }) = self.repo.get(id).await?
        else {
            return Ok(());
        };
        self.authorizer.check(principal, &plan, &note)?;
        if !note.archive_at().is_some_and(|at| at <= now) {
            return Ok(());
        }
        let Some(event) = note.archive(now) else {
            return Ok(());
        };
        let version = self
            .repo
            .save(tx, &note, Expected::Version(version))
            .await?;
        self.publisher
            .publish(tx, events::pending(&note, version, event))
            .await
    }

    /// Saves every note in `tx`, so a failure archives none of them. A note
    /// edited meanwhile is stale and fails it too, to be retried.
    pub async fn archive_notes_by(
        &self,
        principal: &Principal,
        tx: &mut Tx,
        author: AuthorId,
    ) -> AppResult<()> {
        let plan = self.authorize(principal, NoteAction::Archive)?;
        let now = self.clock.now();
        for Versioned {
            entity: mut note,
            version,
        } in self.repo.unarchived_by(author).await?
        {
            self.authorizer.check(principal, &plan, &note)?;
            let Some(event) = note.archive(now) else {
                continue;
            };
            let version = self
                .repo
                .save(tx, &note, Expected::Version(version))
                .await?;
            self.publisher
                .publish(tx, events::pending(&note, version, event))
                .await?;
        }
        Ok(())
    }

    pub async fn get(&self, principal: &Principal, id: NoteId) -> AppResult<Versioned<Note>> {
        let plan = self.authorize(principal, NoteAction::View)?;
        self.load_for(principal, &plan, id).await
    }

    /// The note, if `plan` allows it: checked before any domain action, so a
    /// note the principal may not see is not found, and one they may see but
    /// not change is forbidden.
    async fn load_for(
        &self,
        principal: &Principal,
        plan: &Plan,
        id: NoteId,
    ) -> AppResult<Versioned<Note>> {
        let note = self
            .repo
            .get(id)
            .await?
            .ok_or_else(|| AppError::not_found("note"))?;
        self.authorizer.check(principal, plan, &note.entity)?;
        Ok(note)
    }

    pub async fn list(
        &self,
        principal: &Principal,
        query: ListQuery<NoteSortKey>,
    ) -> AppResult<Listed<Versioned<Note>>> {
        let rows = self.authorizer.rows::<Note>(principal, NoteAction::List)?;
        self.repo.list(&query, &rows).await
    }
}
