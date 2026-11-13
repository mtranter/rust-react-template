#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NoteError {
    #[error("title must not be empty")]
    EmptyTitle,
    #[error("title must be at most {max} characters")]
    TitleTooLong { max: usize },
    #[error("an archived note is read-only")]
    Archived,
    #[error("an item must have text")]
    EmptyItem,
    #[error("an item must be at most {max} characters")]
    ItemTooLong { max: usize },
    #[error("the note already has this item")]
    DuplicateItem,
    #[error("a note holds at most {max} items")]
    TooManyItems { max: usize },
    #[error("the note has no such item")]
    UnknownItem,
}
