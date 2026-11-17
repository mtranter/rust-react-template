#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UserError {
    #[error("email must be a valid address")]
    InvalidEmail,
    #[error("display_name must not be empty")]
    EmptyDisplayName,
}
