//! Commands: the intents the `Note` aggregate's domain actions accept.

use super::{AuthorId, NoteId};

/// Create a new note.
#[derive(Debug, Clone)]
pub struct CreateNote {
    pub author_id: AuthorId,
    pub title: String,
    pub body: String,
}

/// Replace an existing note's title and body.
#[derive(Debug, Clone)]
pub struct EditNote {
    /// The note to edit — used by the service to load the aggregate.
    pub note_id: NoteId,
    pub title: String,
    pub body: String,
}
