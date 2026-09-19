//! Maps domain errors onto the canonical `AppError` taxonomy.

use crate::domain::UserError;
use app_error::AppError;

impl From<UserError> for AppError {
    fn from(err: UserError) -> Self {
        // Exhaustive on purpose: a new domain error must choose its mapping.
        match err {
            UserError::InvalidEmail | UserError::EmptyDisplayName => {
                AppError::validation(err.to_string())
            }
        }
    }
}
