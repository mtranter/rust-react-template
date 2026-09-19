//! Optimistic-concurrency primitives shared by every module.
//!
//! This is a **pure** crate: it names no framework (no poem, no sqlx), so a
//! module's application layer can put these types in its `Repository` trait and
//! its domain stays entirely unaware that aggregates are versioned at all.
//!
//! The vocabulary: a repository's single-item read returns a [`Versioned`]
//! aggregate (the entity plus the [`Version`] it was stored under); a `save`
//! takes an [`Expected`] describing what must already be stored — a fresh insert,
//! or an update that must match a caller-supplied version or be refused as a
//! failed precondition (the row changed underneath the caller).

/// A stored aggregate's optimistic-concurrency version. Freshly created
/// aggregates are [`Version::FIRST`]; every successful write increments it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version(i64);

impl Version {
    /// The version every aggregate has immediately after it is first inserted.
    pub const FIRST: Version = Version(1);

    pub fn from_i64(value: i64) -> Self {
        Version(value)
    }

    pub fn as_i64(self) -> i64 {
        self.0
    }
}

/// An aggregate paired with the version under which it is stored. Single-item
/// reads return this so a later write can prove it read the current state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned<T> {
    pub entity: T,
    pub version: Version,
}

/// What a caller expects to already be stored when saving an aggregate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expected {
    /// A freshly minted aggregate: insert it. There is no prior version.
    New,
    /// An update that must match this stored version, or be refused as a failed
    /// precondition (the row changed underneath the caller).
    Version(Version),
}
