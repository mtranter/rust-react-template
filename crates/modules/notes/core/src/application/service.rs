use super::events;
use super::{
    EventPublisher, Expected, ListQuery, Listed, NoteRepository, NoteSortKey, Transactions, Tx,
    Version, Versioned,
};
use crate::domain::{AuthorId, CreateNote, EditNote, Note, NoteId};
use app_domain::Clock;
use app_error::{AppError, AppResult};
use app_events::EventPayload;
use notes_api::events::v1::{NoteArchived, NoteCreated, NoteEdited};
use std::sync::Arc;
use users_api::UserDirectory;

/// Holds trait objects rather than being generic, so the service is one concrete
/// type that can be named and shared; one dynamic call is negligible beside the I/O.
#[derive(Clone)]
pub struct NoteService {
    repo: Arc<dyn NoteRepository>,
    users: Arc<dyn UserDirectory>,
    publisher: Arc<dyn EventPublisher>,
    transactions: Arc<dyn Transactions>,
    clock: Arc<dyn Clock>,
}

impl NoteService {
    pub fn new(
        repo: Arc<dyn NoteRepository>,
        users: Arc<dyn UserDirectory>,
        publisher: Arc<dyn EventPublisher>,
        transactions: Arc<dyn Transactions>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            users,
            publisher,
            transactions,
            clock,
        }
    }

    /// The author check and the save aren't one transaction: they're different
    /// modules. An author deleted in between is caught by the `UserDeleted`
    /// reaction only if it runs after this commits.
    pub async fn create(&self, cmd: CreateNote) -> AppResult<Versioned<Note>> {
        let author = cmd.author_id;
        let note = Note::create(NoteId::new(), cmd, self.clock.now())?;
        if !self.users.exists(author.as_uuid()).await? {
            return Err(AppError::invalid_field("authorId", "unknown author"));
        }
        let mut tx = self.transactions.begin().await?;
        let version = self.repo.save(&mut tx, &note, Expected::New).await?;
        self.publisher
            .publish::<NoteCreated>(&mut tx, EventPayload::new(events::note(&note)))
            .await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: note,
            version,
        })
    }

    /// `expected` is the version the caller last read (from `If-Match`).
    pub async fn edit(&self, cmd: EditNote, expected: Version) -> AppResult<Versioned<Note>> {
        // Load first so a missing note is a not-found rather than a stale-version
        // conflict; the version check itself is atomic in the repository's update.
        let mut note = self.get(cmd.note_id).await?.entity;
        note.edit(cmd)?;
        let mut tx = self.transactions.begin().await?;
        let version = self
            .repo
            .save(&mut tx, &note, Expected::Version(expected))
            .await?;
        self.publisher
            .publish::<NoteEdited>(&mut tx, EventPayload::new(events::note(&note)))
            .await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: note,
            version,
        })
    }

    /// Saves every note in `tx`, so a failure archives none of them. A note
    /// edited meanwhile is stale and fails it too, to be retried.
    pub async fn archive_notes_by(&self, tx: &mut Tx, author: AuthorId) -> AppResult<()> {
        let now = self.clock.now();
        for Versioned {
            entity: mut note,
            version,
        } in self.repo.unarchived_by(author).await?
        {
            note.archive(now);
            self.repo
                .save(tx, &note, Expected::Version(version))
                .await?;
            self.publisher
                .publish::<NoteArchived>(tx, EventPayload::new(events::note(&note)))
                .await?;
        }
        Ok(())
    }

    pub async fn get(&self, id: NoteId) -> AppResult<Versioned<Note>> {
        self.repo
            .get(id)
            .await?
            .ok_or_else(|| AppError::not_found("note"))
    }

    pub async fn list(&self, query: ListQuery<NoteSortKey>) -> AppResult<Listed<Versioned<Note>>> {
        self.repo.list(&query).await
    }
}
