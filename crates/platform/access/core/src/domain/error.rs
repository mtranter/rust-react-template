#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GrantError {
    #[error("a grant names the role it is for")]
    EmptyRole,
    #[error("a role is at most {max} characters")]
    RoleTooLong { max: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RoleError {
    #[error("a role has a name")]
    EmptyName,
    #[error("a role's name is at most {max} characters")]
    NameTooLong { max: usize },
    #[error("a role's description is at most {max} characters")]
    DescriptionTooLong { max: usize },
    #[error("`admin` and `user` are built in, so they can't be deleted")]
    BuiltIn,
}
