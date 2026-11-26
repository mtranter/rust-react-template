use std::collections::BTreeMap;

/// What an admin changes of a Connection's non-secret values.
#[derive(Debug, Clone, Default)]
pub struct UpdateConnection {
    /// `None` leaves the URL as it is; `Some(None)` clears it.
    pub url: Option<Option<String>>,
    /// Each named field set to its value, or cleared by an empty one; others are left as they are.
    pub fields: BTreeMap<String, String>,
}
