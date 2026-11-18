//! Authorization's engine (ADR 0040): a Module's policy files, checked against its resources' Rust
//! types, and the grants admins make, compiled into one rule set that plans a principal's action
//! as allowed, denied, or a filter on rows. Pure: no poem, no sqlx.

mod authorizer;
mod condition;
mod policies;
mod policy;
mod principal;
mod resource;
mod rule_set;
mod yaml;

#[cfg(test)]
mod tests;

#[cfg(feature = "testing")]
pub mod testing;

pub use authorizer::Authorizer;
pub use condition::Filter;
pub use policies::{
    Catalog, CatalogAction, CatalogResource, GATE_ACTION, Grant, GrantRefusal, GrantRefused,
    Policies, PoliciesBuilder, PolicyFile, Scope,
};
pub use principal::{Principal, Value};
pub use resource::{Action, Resource};
pub use rule_set::{CompileError, Compiled, Plan, Rows, RuleSet};

/// A policy file refused when loading, naming its file and line.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{file}:{line}: {reason}")]
pub struct PolicyError {
    pub file: String,
    pub line: usize,
    pub reason: String,
}
