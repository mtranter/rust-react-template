//! The users domain error.

/// Why a users domain action was refused. Carries domain meaning only — the
/// application layer maps each variant onto the canonical `AppError` taxonomy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UserError {
    #[error("email must be a valid address")]
    InvalidEmail,
    #[error("display_name must not be empty")]
    EmptyDisplayName,
}
