use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A read model, not an aggregate: assembled by a query, never saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSummary {
    pub id: Uuid,
    pub title: String,
    pub author_id: Uuid,
    /// `None` when no user has the note's author id: notes holds no foreign key
    /// to users (spec D1).
    pub author_name: Option<String>,
    pub created_at: DateTime<Utc>,
}
