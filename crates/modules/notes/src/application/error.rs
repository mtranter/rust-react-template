//! Maps domain errors onto the canonical `AppError` taxonomy.

use crate::domain::NoteError;
use app_error::AppError;

impl From<NoteError> for AppError {
    fn from(err: NoteError) -> Self {
        // Exhaustive on purpose: a new domain error must choose its mapping.
        match err {
            NoteError::EmptyTitle | NoteError::TitleTooLong { .. } => {
                AppError::validation(err.to_string())
            }
        }
    }
}
