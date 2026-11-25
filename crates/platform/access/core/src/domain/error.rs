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

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssignmentError {
    #[error("an assignment names the subject (the token's `sub`) it is for")]
    EmptySubject,
    #[error("a subject is at most {max} characters")]
    SubjectTooLong { max: usize },
    #[error("an assignment names the role it assigns")]
    EmptyRole,
    #[error("every signed-in user holds `user`, so it is never assigned or unassigned")]
    Implicit,
    #[error(
        "the subject is a bootstrap admin (`APP_BOOTSTRAP_ADMINS`), so it always holds `admin`"
    )]
    BootstrapAdmin,
    #[error(
        "the last `admin` can't be unassigned while there are no bootstrap admins: assign another first"
    )]
    LastAdmin,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvitationError {
    #[error("that isn't an email address")]
    InvalidEmail,
    #[error("a name is at most {max} characters")]
    NameTooLong { max: usize },
    #[error("{0}")]
    Role(AssignmentError),
    #[error("an invitation to that email is pending already: resend it instead")]
    AlreadyPending,
    #[error("that email belongs to someone who has set their password: assign them roles instead")]
    AlreadyActive,
    #[error("the invitation was accepted: the invitee has signed in")]
    Accepted,
    #[error("the invitation was revoked")]
    Revoked,
    #[error(
        "the invitee has set their password, so the invitation can't be revoked: unassign their roles instead"
    )]
    PasswordSet,
}
