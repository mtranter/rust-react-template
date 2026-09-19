//! The persistence interface owned by the application layer.

use super::{Expected, Version, Versioned};
use crate::domain::{Note, NoteId};
use app_error::AppResult;

/// Loads and saves `Note` aggregates. Speaks in aggregates, never rows.
///
/// Single-item loads return the aggregate paired with its stored [`Version`], so
/// a subsequent [`save`](Self::save) can prove which state it read and be
/// refused if the row has since changed (optimistic concurrency).
#[async_trait::async_trait]
pub trait NoteRepository: Send + Sync {
    async fn get(&self, id: NoteId) -> AppResult<Option<Versioned<Note>>>;
    /// Returns a page of notes, most-recent first, plus the total count.
    async fn list(&self, limit: i64, offset: i64) -> AppResult<(Vec<Versioned<Note>>, i64)>;
    /// Persists a note. `expected` says what must already be stored: [`Expected::New`]
    /// inserts; [`Expected::Version`] updates only if the stored version matches,
    /// returning a `PreconditionFailed` error otherwise. Returns the new version.
    async fn save(&self, note: &Note, expected: Expected) -> AppResult<Version>;
}
