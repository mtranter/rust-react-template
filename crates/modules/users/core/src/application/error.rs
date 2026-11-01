use crate::domain::UserError;
use app_error::AppError;

impl From<UserError> for AppError {
    fn from(err: UserError) -> Self {
        // Exhaustive on purpose: a new domain error must choose its mapping.
        match err {
            UserError::InvalidEmail => AppError::invalid_field("email", err.to_string()),
            UserError::EmptyDisplayName => AppError::invalid_field("displayName", err.to_string()),
        }
    }
}
