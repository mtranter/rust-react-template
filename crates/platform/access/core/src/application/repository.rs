use super::{ListQuery, Listed, Tx, Version, Versioned};
use crate::domain::{Grant, GrantId};
use app_authz::Rows;
use app_error::AppResult;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantSortKey {
    Role,
    Resource,
    Action,
    GrantedAt,
}

/// Each given field matches exactly.
#[derive(Debug, Clone, Default)]
pub struct GrantFilter {
    pub role: Option<String>,
    pub resource: Option<String>,
    pub action: Option<String>,
}

/// Grants are made and revoked, never changed, so a grant is saved once at version 1. Every
/// change to them bumps the revision in its own unit of work, which is what server instances
/// poll to know their rule set is stale.
#[async_trait::async_trait]
pub trait GrantRepository: Send + Sync {
    async fn get(&self, id: GrantId) -> AppResult<Option<Versioned<Grant>>>;
    /// Ordered by resource, action and role unless sorted otherwise.
    async fn list(
        &self,
        filter: &GrantFilter,
        query: &ListQuery<GrantSortKey>,
        rows: &Rows,
    ) -> AppResult<Listed<Versioned<Grant>>>;
    /// Every grant, for the rule set.
    async fn all(&self) -> AppResult<Vec<Grant>>;
    /// `Conflict` when an equal grant exists.
    async fn insert(&self, tx: &mut Tx, grant: &Grant) -> AppResult<Version>;
    async fn exists(&self, tx: &mut Tx, rule: &app_authz::Grant) -> AppResult<bool>;
    /// Only if the stored version is `expected`, else `PreconditionFailed`.
    async fn delete(&self, tx: &mut Tx, id: GrantId, expected: Version) -> AppResult<()>;

    async fn revision(&self) -> AppResult<i64>;
    /// Holds the revision's row until `tx` ends, so changes to the grants commit one at a time.
    async fn bump_revision(&self, tx: &mut Tx) -> AppResult<i64>;

    /// The keys of the default grants applied so far: the module whose they are, and the grant.
    async fn applied_defaults(&self, tx: &mut Tx) -> AppResult<Vec<(String, app_authz::Grant)>>;
    async fn record_default(
        &self,
        tx: &mut Tx,
        module: &str,
        rule: &app_authz::Grant,
        at: DateTime<Utc>,
    ) -> AppResult<()>;
}
