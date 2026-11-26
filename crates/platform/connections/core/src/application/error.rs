use crate::domain::ConnectionError;
use app_error::AppError;

impl From<ConnectionError> for AppError {
    fn from(err: ConnectionError) -> Self {
        // Exhaustive on purpose: a new domain error must choose its mapping.
        match err {
            ConnectionError::InvalidKey(_) => AppError::not_found("connection"),
            ConnectionError::UnknownSecret(_) => AppError::not_found("secret"),
            ConnectionError::InvalidUrl { .. } => AppError::invalid_field("url", err.to_string()),
            ConnectionError::UnknownField(ref name)
            | ConnectionError::FieldTooLong { ref name, .. } => {
                AppError::invalid_field(format!("fields.{name}"), err.to_string())
            }
            ConnectionError::EmptySecret | ConnectionError::SecretTooLong { .. } => {
                AppError::invalid_field("value", err.to_string())
            }
            // A module's declaration, refused as `migrate` records it: a mistake in code.
            ConnectionError::EmptyLabel(_)
            | ConnectionError::DuplicateField { .. }
            | ConnectionError::UnnamedField { .. }
            | ConnectionError::UnnamedApiKey(_)
            | ConnectionError::DeclaredTwice(_) => AppError::validation(err.to_string()),
        }
    }
}
