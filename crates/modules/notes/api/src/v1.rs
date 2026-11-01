//! The rows of the `notes_api.*_v1` published views (spec D8).

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A row of `notes_api.note_v1`. Only ever gains fields, so consumers never
/// build one: they get it from [`crate::seed`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Note {
    pub id: Uuid,
    pub author_id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
}
