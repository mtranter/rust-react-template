//! Optimistic concurrency, kept out of the domain: repositories wrap aggregates
//! in [`Versioned`] and `save` takes an [`Expected`].

/// Every successful write increments it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version(i64);

impl Version {
    pub const FIRST: Version = Version(1);

    pub fn from_i64(value: i64) -> Self {
        Version(value)
    }

    pub fn as_i64(self) -> i64 {
        self.0
    }

    /// What deleting this version announces: strictly the newest of its root.
    pub fn next(self) -> Self {
        Version(self.0 + 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned<T> {
    pub entity: T,
    pub version: Version,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expected {
    New,
    /// Refused as a failed precondition if the stored version differs.
    Version(Version),
}
