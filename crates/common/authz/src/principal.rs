use std::collections::BTreeMap;
use std::fmt;

/// An attribute's value: of a resource, a principal, or a literal in a condition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Value {
    String(String),
    Integer(i64),
    Boolean(bool),
    Null,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "{s:?}"),
            Value::Integer(i) => write!(f, "{i}"),
            Value::Boolean(b) => write!(f, "{b}"),
            Value::Null => f.write_str("null"),
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_owned())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Integer(i)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Boolean(b)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Self {
        v.map_or(Value::Null, Into::into)
    }
}

/// Who is acting (ADR 0040).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    User {
        id: String,
        roles: Vec<String>,
        attr: BTreeMap<String, Value>,
    },
    /// The worker's and seeds' caller, acting on facts already decided: policies don't apply.
    System,
}

impl Principal {
    pub fn user(id: impl Into<String>, roles: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Principal::User {
            id: id.into(),
            roles: roles.into_iter().map(Into::into).collect(),
            attr: BTreeMap::new(),
        }
    }

    /// The user's id; `None` for `System`.
    pub fn id(&self) -> Option<&str> {
        match self {
            Principal::User { id, .. } => Some(id),
            Principal::System => None,
        }
    }
}
