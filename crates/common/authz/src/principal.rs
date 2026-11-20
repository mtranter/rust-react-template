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

/// The agent acting for a user, or for no one when its id is the user's own (ADR 0025).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub id: String,
    /// The agents behind it, from the outer to the inner: who it in turn acts for, as the token's
    /// nested `act` claims name them (RFC 8693 §4.1).
    pub chain: Vec<String>,
}

impl Actor {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            chain: Vec::new(),
        }
    }

    pub fn behind(self, chain: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            chain: chain.into_iter().map(Into::into).collect(),
            ..self
        }
    }
}

/// Who is acting (ADR 0040).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    User {
        id: String,
        roles: Vec<String>,
        attr: BTreeMap<String, Value>,
        /// The agent acting, if any; `None` when the user acts themselves.
        actor: Option<Actor>,
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
            actor: None,
        }
    }

    /// The same Principal, with `actor` acting; `System` stays as it is.
    pub fn acting(mut self, actor: Option<Actor>) -> Self {
        if let Principal::User { actor: acting, .. } = &mut self {
            *acting = actor;
        }
        self
    }

    /// The agent acting, if any.
    pub fn actor(&self) -> Option<&Actor> {
        match self {
            Principal::User { actor, .. } => actor.as_ref(),
            Principal::System => None,
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
