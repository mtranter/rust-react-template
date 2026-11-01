use super::{Expected, ListQuery, Listed, Version, Versioned};
use crate::domain::{Note, NoteId};
use app_error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteSortKey {
    Title,
    CreatedAt,
}

/// Single-item loads pair the aggregate with its stored [`Version`] so a later
/// [`save`](Self::save) is refused if the row has changed since (optimistic concurrency).
#[async_trait::async_trait]
pub trait NoteRepository: Send + Sync {
    async fn get(&self, id: NoteId) -> AppResult<Option<Versioned<Note>>>;
    /// Most-recent first unless sorted otherwise.
    async fn list(&self, query: &ListQuery<NoteSortKey>) -> AppResult<Listed<Versioned<Note>>>;
    /// [`Expected::Version`] updates only if the stored version matches, else
    /// `PreconditionFailed`. Returns the new version.
    async fn save(&self, note: &Note, expected: Expected) -> AppResult<Version>;
}
