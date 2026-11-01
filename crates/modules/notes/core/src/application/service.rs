use super::{Expected, ListQuery, Listed, NoteRepository, NoteSortKey, Version, Versioned};
use crate::domain::{CreateNote, EditNote, Note, NoteId};
use app_error::{AppError, AppResult};
use std::sync::Arc;
use users_api::UserDirectory;

/// Holds trait objects rather than being generic, so the service is one concrete
/// type that can be named and shared; one dynamic call is negligible beside the I/O.
#[derive(Clone)]
pub struct NoteService {
    repo: Arc<dyn NoteRepository>,
    users: Arc<dyn UserDirectory>,
}

impl NoteService {
    pub fn new(repo: Arc<dyn NoteRepository>, users: Arc<dyn UserDirectory>) -> Self {
        Self { repo, users }
    }

    pub async fn create(&self, cmd: CreateNote) -> AppResult<Versioned<Note>> {
        let author = cmd.author_id;
        let note = Note::create(NoteId::new(), cmd, app_domain::now())?;
        if !self.users.exists(author.as_uuid()).await? {
            return Err(AppError::invalid_field("authorId", "unknown author"));
        }
        let version = self.repo.save(&note, Expected::New).await?;
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
        let version = self.repo.save(&note, Expected::Version(expected)).await?;
        Ok(Versioned {
            entity: note,
            version,
        })
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
