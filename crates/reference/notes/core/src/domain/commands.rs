use super::{AuthorId, NoteId};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct CreateNote {
    pub author_id: AuthorId,
    pub title: String,
    pub body: String,
    pub archive_at: Option<DateTime<Utc>>,
}

/// Replaces what a note's author chooses: its title, body and when it archives.
#[derive(Debug, Clone)]
pub struct EditNote {
    pub note_id: NoteId,
    pub title: String,
    pub body: String,
    pub archive_at: Option<DateTime<Utc>>,
}
