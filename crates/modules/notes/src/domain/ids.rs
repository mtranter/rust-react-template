//! Strongly-typed identifiers.

use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

macro_rules! uuid_newtype {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(Uuid);

        impl $name {
            /// Wrap an existing `Uuid` (e.g. read back from the database).
            pub const fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }

            /// The underlying `Uuid`, for the persistence boundary.
            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::parse_str(s)?))
            }
        }

        impl From<Uuid> for $name {
            fn from(id: Uuid) -> Self {
                Self(id)
            }
        }
    };
}

uuid_newtype!(
    /// Strongly-typed identifier for a [`Note`](super::Note).
    NoteId
);

impl NoteId {
    /// Mint a fresh, random id. Called by the application layer when creating an
    /// aggregate; seeds use well-known ids via `from_uuid` instead.
    #[allow(clippy::new_without_default)] // a random `default()` would be misleading
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

uuid_newtype!(
    /// Opaque reference to the principal that authored a note. Deliberately the
    /// notes module's own type — it does not depend on `users::UserId`.
    AuthorId
);
