use chrono::{DateTime, Utc};

/// A read model, not an aggregate: one root of the projected topic as this
/// module's copy holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The root's key in its topic: its id.
    pub key: String,
    /// When the copy last changed: the time of the latest event applied.
    pub updated_at: DateTime<Utc>,
}
