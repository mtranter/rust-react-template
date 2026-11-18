use super::{ListQuery, Listed, Tx, Version, Versioned};
use crate::domain::{Assignment, Grant, GrantId, Role};
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

    /// The revision of grants, roles and assignments alike: a change to any bumps it.
    async fn revision(&self) -> AppResult<i64>;
    /// Holds the revision's row until `tx` ends, so changes to grants, roles and assignments
    /// commit one at a time.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleSortKey {
    Name,
    CreatedAt,
}

/// Roles are created and deleted, never changed. A role created again under a deleted one's
/// name continues from the version its deletion reached, since its events are keyed by name.
#[async_trait::async_trait]
pub trait RoleRepository: Send + Sync {
    async fn get(&self, name: &str) -> AppResult<Option<Versioned<Role>>>;
    /// Ordered by name unless sorted otherwise.
    async fn list(
        &self,
        query: &ListQuery<RoleSortKey>,
        rows: &Rows,
    ) -> AppResult<Listed<Versioned<Role>>>;
    /// `Conflict` when a role of that name exists.
    async fn insert(&self, tx: &mut Tx, role: &Role) -> AppResult<Version>;
    async fn exists(&self, tx: &mut Tx, name: &str) -> AppResult<bool>;
    /// Only if the stored version is `expected`, else `PreconditionFailed`; `Conflict` while a
    /// grant names the role.
    async fn delete(&self, tx: &mut Tx, name: &str, expected: Version) -> AppResult<()>;
}

/// Assignments are made and removed, never changed. One assigned again after its removal
/// continues from the version its removal reached, since its events are keyed by subject and role.
#[async_trait::async_trait]
pub trait AssignmentRepository: Send + Sync {
    async fn get(&self, subject: &str, role: &str) -> AppResult<Option<Versioned<Assignment>>>;
    /// Every assignment, as (subject, role), for the rule set.
    async fn all(&self) -> AppResult<Vec<(String, String)>>;
    /// `Conflict` when it exists; invalid on `role` when no role has that name.
    async fn insert(&self, tx: &mut Tx, assignment: &Assignment) -> AppResult<Version>;
    /// How many subjects `role` is assigned to.
    async fn holders(&self, tx: &mut Tx, role: &str) -> AppResult<i64>;
    /// Only if the stored version is `expected`, else `PreconditionFailed`.
    async fn delete(
        &self,
        tx: &mut Tx,
        subject: &str,
        role: &str,
        expected: Version,
    ) -> AppResult<()>;
}
