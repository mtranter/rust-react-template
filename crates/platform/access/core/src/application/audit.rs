use super::{AccessService, ListQuery, Listed};
use app_authz::{Plan, Principal, Resource, Value};
use app_error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// What changed in an [`AuditEntry`]: one per event of the grant, role and assignment topics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum AuditChange {
    AccessGranted,
    GrantRevoked,
    RoleCreated,
    RoleDeleted,
    RoleAssigned,
    RoleUnassigned,
}

/// A read model of the audit log, not an aggregate: copied from `access`' own topics, never
/// saved by a use case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    /// The event's id.
    pub id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub change: AuditChange,
    /// `None` for `System`: `migrate` applying a default grant, or a seed.
    pub actor: Option<String>,
    pub role: String,
    /// The assignment's subject.
    pub subject: Option<String>,
    /// The grant's resource, action and scope.
    pub resource: Option<String>,
    pub action: Option<String>,
    pub scope: Option<String>,
}

/// What can be done with the audit log: `admin`'s by `access`' own policies, as with grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum AuditEntryAction {
    List,
}

impl Resource for AuditEntry {
    const KIND: &'static str = "auditEntry";
    const ATTRS: &'static [&'static str] = &[];
    type Action = AuditEntryAction;
    const VIEW: Option<AuditEntryAction> = Some(AuditEntryAction::List);

    fn attr(&self, _: &str) -> Option<Value> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditSortKey {
    OccurredAt,
}

/// Each given field matches exactly; `since` and `until` bound `occurred_at`, inclusive and
/// exclusive.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub actor: Option<String>,
    pub role: Option<String>,
    pub subject: Option<String>,
    pub change: Option<AuditChange>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

/// The read side of the audit log: answers from its copy, never loads an aggregate.
#[async_trait::async_trait]
pub trait AuditLog: Send + Sync {
    /// Newest first unless sorted otherwise.
    async fn find(
        &self,
        filter: &AuditFilter,
        query: &ListQuery<AuditSortKey>,
    ) -> AppResult<Listed<AuditEntry>>;
}

impl AccessService {
    /// The copy has no columns for a scope to filter by, so a scoped plan is refused rather
    /// than shown entries the caller may not see.
    pub async fn list_audit(
        &self,
        principal: &Principal,
        filter: AuditFilter,
        query: ListQuery<AuditSortKey>,
    ) -> AppResult<Listed<AuditEntry>> {
        if self
            .authorizer
            .authorize::<AuditEntry>(principal, AuditEntryAction::List)?
            != Plan::Allowed
        {
            return Err(AppError::Forbidden);
        }
        self.audit.find(&filter, &query).await
    }
}
