//! `NoteService`: the notes use cases.

use super::{Expected, NoteRepository, Version, Versioned};
use crate::domain::{CreateNote, EditNote, Note, NoteId};
use app_error::{AppError, AppResult};
use chrono::{DateTime, SubsecRound, Utc};
use std::sync::Arc;

/// Coordinates the notes use cases over a [`NoteRepository`]. Supplies what the
/// domain must not produce itself: fresh identities and the current time.
///
/// Holds the repository as a trait object rather than being generic over it, so
/// the service is one concrete type: it can be named, stored, and shared without
/// threading a type parameter through every caller. The cost is one dynamic call
/// per repository method, negligible beside the I/O behind it.
#[derive(Clone)]
pub struct NoteService {
    repo: Arc<dyn NoteRepository>,
}

impl NoteService {
    pub fn new(repo: Arc<dyn NoteRepository>) -> Self {
        Self { repo }
    }

    pub async fn create(&self, cmd: CreateNote) -> AppResult<Versioned<Note>> {
        let note = Note::create(NoteId::new(), cmd, now())?;
        let version = self.repo.save(&note, Expected::New).await?;
        Ok(Versioned {
            entity: note,
            version,
        })
    }

    /// Edit a note: load, act, save. `expected` is the version the caller last
    /// read (from `If-Match`); the save is refused if the note has since changed.
    pub async fn edit(&self, cmd: EditNote, expected: Version) -> AppResult<Versioned<Note>> {
        // Load first so a missing note is a clean not-found, distinct from a
        // stale-version conflict. The concurrency check itself is the caller's
        // `expected`, enforced atomically by the repository's conditional update.
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

    pub async fn list(&self, limit: i64, offset: i64) -> AppResult<(Vec<Versioned<Note>>, i64)> {
        self.repo.list(limit, offset).await
    }
}

/// The current time at Postgres' `timestamptz` precision (microseconds), so an
/// aggregate returned straight after `save` matches the same one read back.
fn now() -> DateTime<Utc> {
    Utc::now().trunc_subsecs(6)
}
