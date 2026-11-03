#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NoteError {
    #[error("title must not be empty")]
    EmptyTitle,
    #[error("title must be at most {max} characters")]
    TitleTooLong { max: usize },
    #[error("an archived note is read-only")]
    Archived,
}
