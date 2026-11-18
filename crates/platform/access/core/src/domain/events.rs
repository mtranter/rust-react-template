/// What happened to a [`Grant`](super::Grant); the service announces each one as the matching
/// `access_api` event.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantEvent {
    Granted,
    Revoked { by: super::GrantedBy },
}

/// What happened to a [`Role`](super::Role).
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleEvent {
    Created,
    Deleted { by: super::GrantedBy },
}
