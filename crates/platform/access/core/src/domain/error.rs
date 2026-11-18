#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GrantError {
    #[error("a grant names the role it is for")]
    EmptyRole,
    #[error("a role is at most {max} characters")]
    RoleTooLong { max: usize },
}
