use crate::domain::{AssignmentError, GrantError, RoleError};
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

impl From<RoleError> for AppError {
    fn from(err: RoleError) -> Self {
        match err {
            RoleError::EmptyName | RoleError::NameTooLong { .. } => {
                AppError::invalid_field("name", err.to_string())
            }
            RoleError::DescriptionTooLong { .. } => {
                AppError::invalid_field("description", err.to_string())
            }
            RoleError::BuiltIn => AppError::conflict(err.to_string()),
        }
    }
}

impl From<AssignmentError> for AppError {
    fn from(err: AssignmentError) -> Self {
        match err {
            AssignmentError::EmptySubject | AssignmentError::SubjectTooLong { .. } => {
                AppError::invalid_field("subject", err.to_string())
            }
            AssignmentError::EmptyRole | AssignmentError::Implicit => {
                AppError::invalid_field("role", err.to_string())
            }
            AssignmentError::BootstrapAdmin | AssignmentError::LastAdmin => {
                AppError::conflict(err.to_string())
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
        GrantRefusal::UnknownScope { .. } => "scope",
    };
    AppError::invalid_field(field, refused.reason.to_string())
}
