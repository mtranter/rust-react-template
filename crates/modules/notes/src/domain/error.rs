//! The notes domain error.

/// Why a notes domain action was refused. The application layer maps each
/// variant onto the canonical `AppError` taxonomy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NoteError {
    #[error("title must not be empty")]
    EmptyTitle,
    #[error("title must be at most {max} characters")]
    TitleTooLong { max: usize },
}
