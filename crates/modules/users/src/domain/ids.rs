//! Strongly-typed identifiers.

use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Strongly-typed identifier for a [`User`](super::User).
///
/// A newtype over `Uuid` so a user id can never be mixed up with any other id at
/// a call site. Kept pure (no sqlx traits): the db layer converts to/from `Uuid`
/// at its boundary via [`UserId::as_uuid`] / [`UserId::from_uuid`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(Uuid);

impl UserId {
    /// Mint a fresh, random id. Called by the application layer when creating an
    /// aggregate; seeds use well-known ids via [`UserId::from_uuid`] instead.
    #[allow(clippy::new_without_default)] // a random `default()` would be misleading
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wrap an existing `Uuid` (e.g. a value read back from the database).
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// The underlying `Uuid`, for the persistence boundary.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for UserId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for UserId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}
