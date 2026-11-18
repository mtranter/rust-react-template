use crate::domain::GrantError;
use app_authz::{GrantRefusal, GrantRefused};
use app_error::AppError;

impl From<GrantError> for AppError {
    fn from(err: GrantError) -> Self {
        // Exhaustive on purpose: a new domain error must choose its mapping.
        match err {
            GrantError::EmptyRole | GrantError::RoleTooLong { .. } => {
                AppError::invalid_field("role", err.to_string())
            }
        }
    }
}

/// A grant the loaded policies don't have, pinned to the field at fault.
pub(crate) fn refused(refused: GrantRefused) -> AppError {
    let field = match &refused.reason {
        GrantRefusal::NoRole => "role",
        GrantRefusal::UnknownResource(_) => "resource",
        GrantRefusal::UnknownAction { .. } => "action",
        GrantRefusal::UnknownScope { .. } | GrantRefusal::ScopeOnGate => "scope",
    };
    AppError::invalid_field(field, refused.reason.to_string())
}
