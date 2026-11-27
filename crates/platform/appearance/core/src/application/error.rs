use crate::domain::ThemeError;
use app_error::{AppError, FieldError};

impl From<ThemeError> for AppError {
    fn from(err: ThemeError) -> Self {
        AppError::Validation {
            message: err.to_string(),
            fields: err
                .faults
                .into_iter()
                .map(|(path, message)| FieldError { path, message })
                .collect(),
        }
    }
}
