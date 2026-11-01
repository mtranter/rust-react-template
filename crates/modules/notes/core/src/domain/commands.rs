use super::{AuthorId, NoteId};

#[derive(Debug, Clone)]
pub struct CreateNote {
    pub author_id: AuthorId,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct EditNote {
    pub note_id: NoteId,
    pub title: String,
    pub body: String,
}
