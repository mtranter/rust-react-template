use crate::domain::WidgetError;
use app_error::AppError;

impl From<WidgetError> for AppError {
    fn from(err: WidgetError) -> Self {
        // Exhaustive on purpose: a new domain error must choose its mapping.
        match err {
            WidgetError::EmptyName | WidgetError::NameTooLong { .. } => {
                AppError::invalid_field("name", err.to_string())
            }
        }
    }
}
