use super::error::refused;
use super::events::{self, scope_name};
use super::{
    AssignmentRepository, CatalogAction, CatalogCapability, CatalogResource, EventPublisher,
    GrantFilter, GrantRepository, GrantSortKey, IdentityDirectory, InvitationRepository,
    ListQuery, Listed, RoleRepository, SubjectRepository, Transactions, Version, Versioned,
};
use crate::domain::{
    CreateRole, Grant, GrantAccess, GrantAction, GrantEvent, GrantId, GrantedBy, Role,
};
use app_authz::{Authorizer, Plan, Policies, Principal, check_grant};
use app_domain::Clock;
use app_error::{AppError, AppResult};
use std::collections::HashSet;
use std::sync::Arc;

/// An operation and the action it performs, from its module's `OPERATIONS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Operation {
    pub id: &'static str,
    pub resource: &'static str,
    pub action: &'static str,
}

/// Every module served beside `access`, its own included: what can be granted, and by which
/// operations it is performed.
#[derive(Debug, Clone)]
pub struct Modules {
    pub policies: Vec<Policies>,
    pub operations: Vec<Operation>,
}

/// An action on a resource a principal may perform on some row.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Permission {
    pub resource: String,
    pub action: String,
}

/// A grant as an admin lists it: stale when its module's policies no longer have its resource,
/// action or scope, so the rule set skips it.
#[derive(Debug, Clone)]
pub struct Listing {
    pub grant: Versioned<Grant>,
    pub stale: bool,
}

#[derive(Clone)]
pub struct AccessService {
    pub(super) repo: Arc<dyn GrantRepository>,
    pub(super) roles: Arc<dyn RoleRepository>,
    pub(super) assignments: Arc<dyn AssignmentRepository>,
    pub(super) subjects: Arc<dyn SubjectRepository>,
    pub(super) invitations: Arc<dyn InvitationRepository>,
    /// `None` when no identity provider is configured: inviting is off.
    pub(super) directory: Option<Arc<dyn IdentityDirectory>>,
    pub(super) publisher: Arc<dyn EventPublisher>,
    pub(super) transactions: Arc<dyn Transactions>,
    pub(super) clock: Arc<dyn Clock>,
    pub(super) authorizer: Arc<Authorizer>,
    pub(super) modules: Arc<Modules>,
}

pub(super) fn acting(principal: &Principal) -> GrantedBy {
    match principal.id() {
        Some(id) => GrantedBy::User(id.to_owned()),
        None => GrantedBy::System,
    }
}

impl AccessService {
    // One argument per port `di.rs` wires.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: Arc<dyn GrantRepository>,
        roles: Arc<dyn RoleRepository>,
        assignments: Arc<dyn AssignmentRepository>,
        subjects: Arc<dyn SubjectRepository>,
        invitations: Arc<dyn InvitationRepository>,
        directory: Option<Arc<dyn IdentityDirectory>>,
        publisher: Arc<dyn EventPublisher>,
        transactions: Arc<dyn Transactions>,
        clock: Arc<dyn Clock>,
        authorizer: Arc<Authorizer>,
        modules: Arc<Modules>,
    ) -> Self {
        Self {
            repo,
            roles,
            assignments,
            subjects,
            invitations,
            directory,
            publisher,
            transactions,
            clock,
            authorizer,
            modules,
        }
    }

    fn authorize(&self, principal: &Principal, action: GrantAction) -> AppResult<Plan> {
        self.authorizer.authorize::<Grant>(principal, action)
    }

    fn is_stale(&self, grant: &Grant) -> bool {
        check_grant(&self.modules.policies, grant.rule()).is_err()
    }

    pub async fn list(
        &self,
        principal: &Principal,
        filter: GrantFilter,
        query: ListQuery<GrantSortKey>,
    ) -> AppResult<Listed<Listing>> {
        let rows = self
            .authorizer
            .rows::<Grant>(principal, GrantAction::List)?;
        let listed = self.repo.list(&filter, &query, &rows).await?;
        Ok(Listed {
            items: listed
                .items
                .into_iter()
                .map(|grant| Listing {
                    stale: self.is_stale(&grant.entity),
                    grant,
                })
                .collect(),
            total: listed.total,
        })
    }

    /// Checked against the loaded policies, so a stored grant always compiles; one naming a role
    /// that doesn't exist is refused by the store, and one equal to an existing grant is a
    /// conflict. Takes effect on each server instance at its next poll.
    pub async fn grant(
        &self,
        principal: &Principal,
        cmd: GrantAccess,
    ) -> AppResult<Versioned<Grant>> {
        self.authorize(principal, GrantAction::Create)?;
        let (grant, event) = Grant::make(GrantId::new(), cmd, acting(principal), self.clock.now())?;
        check_grant(&self.modules.policies, grant.rule()).map_err(refused)?;
        let mut tx = self.transactions.begin().await?;
        let version = self.repo.insert(&mut tx, &grant).await?;
        self.publisher
            .publish(&mut tx, events::pending(&grant, version, event))
            .await?;
        self.repo.bump_revision(&mut tx).await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: grant,
            version,
        })
    }

    /// Removes the grant and announces it, as it was last, in the same transaction.
    pub async fn revoke(
        &self,
        principal: &Principal,
        id: GrantId,
        expected: Version,
    ) -> AppResult<()> {
        let plan = self.authorize(principal, GrantAction::Revoke)?;
        let current = self
            .repo
            .get(id)
            .await?
            .ok_or_else(|| AppError::not_found("grant"))?;
        self.authorizer.check(principal, &plan, &current.entity)?;
        if current.version != expected {
            return Err(AppError::precondition_failed(
                "the grant was revoked and granted again meanwhile",
            ));
        }
        let event = current.entity.revoke(acting(principal));
        let mut tx = self.transactions.begin().await?;
        self.repo.delete(&mut tx, id, expected).await?;
        self.publisher
            .publish(
                &mut tx,
                events::pending(&current.entity, expected.next(), event),
            )
            .await?;
        self.repo.bump_revision(&mut tx).await?;
        tx.commit().await
    }

    /// Every Capability, in the order its first Module is registered, with its Modules'
    /// resources, their actions with the operations performing each and whether anything in force
    /// could allow them, and their scopes, each labelled for admins.
    pub fn catalog(&self, principal: &Principal) -> AppResult<Vec<CatalogCapability>> {
        self.authorize(principal, GrantAction::List)?;
        let rule_set = self.authorizer.current();
        let mut capabilities: Vec<CatalogCapability> = Vec::new();
        for policies in &self.modules.policies {
            let catalog = policies.catalog(&rule_set);
            let resources = catalog
                .resources
                .into_iter()
                .map(|resource| CatalogResource {
                    module: catalog.module.clone(),
                    module_label: catalog.label.clone(),
                    label: resource.label,
                    actions: resource
                        .actions
                        .into_iter()
                        .map(|action| CatalogAction {
                            operations: self
                                .modules
                                .operations
                                .iter()
                                .filter(|o| o.resource == resource.kind && o.action == action.name)
                                .map(|o| o.id)
                                .collect(),
                            name: action.name,
                            label: action.label,
                            reachable: action.reachable,
                        })
                        .collect(),
                    resource: resource.kind,
                    scopes: resource.scopes,
                });
            match capabilities
                .iter_mut()
                .find(|c| c.capability == catalog.capability.id)
            {
                Some(capability) => capability.resources.extend(resources),
                None => capabilities.push(CatalogCapability {
                    capability: catalog.capability.id,
                    label: catalog.capability.label,
                    resources: resources.collect(),
                }),
            }
        }
        Ok(capabilities)
    }

    /// Every resource and action the rule set in force lets the principal perform on some row:
    /// the plan the server enforces, so built-in rules count as grants do. Asks no action of its
    /// own, as it answers only for the caller.
    pub fn permissions(&self, principal: &Principal) -> Vec<Permission> {
        let rule_set = self.authorizer.current();
        let mut permissions: Vec<Permission> = self
            .modules
            .policies
            .iter()
            .flat_map(|policies| policies.catalog(&rule_set).resources)
            .flat_map(|resource| {
                resource.actions.into_iter().map(move |action| Permission {
                    resource: resource.kind.clone(),
                    action: action.name,
                })
            })
            .filter(|p| rule_set.plan_named(principal, &p.resource, &p.action) != Plan::Denied)
            .collect();
        permissions.sort();
        permissions.dedup();
        permissions
    }

    /// Grants, as `System`, every module's default grant not applied before, in one unit of
    /// work: what `migrate` runs once every module is migrated. A default an admin revoked was
    /// applied, so it isn't granted again; one equal to a grant an admin made is recorded only.
    /// A role a default being granted names is created first if it doesn't exist; a role an admin
    /// deleted after its defaults were applied isn't, as they aren't granted again. Returns how
    /// many it granted.
    pub async fn apply_defaults(&self) -> AppResult<usize> {
        let mut tx = self.transactions.begin().await?;
        // Taken first, so two `migrate`s apply the defaults one after the other.
        self.repo.bump_revision(&mut tx).await?;
        let mut applied: HashSet<_> = self
            .repo
            .applied_defaults(&mut tx)
            .await?
            .into_iter()
            .collect();
        let now = self.clock.now();
        let mut granted = 0;
        let mut recorded = 0;
        for policies in &self.modules.policies {
            for rule in policies.default_grants() {
                let key = (policies.module().to_owned(), rule.clone());
                // A module's file may name one grant twice (`*` and the action itself).
                if !applied.insert(key) {
                    continue;
                }
                self.repo
                    .record_default(&mut tx, policies.module(), rule, now)
                    .await?;
                recorded += 1;
                if self.repo.exists(&mut tx, rule).await? {
                    continue;
                }
                if !self.roles.exists(&mut tx, &rule.role).await? {
                    let (role, event) = Role::create(
                        CreateRole {
                            name: rule.role.clone(),
                            description: format!(
                                "Named by the default grants of `{}`.",
                                policies.module()
                            ),
                        },
                        GrantedBy::System,
                        now,
                    )?;
                    let version = self.roles.insert(&mut tx, &role).await?;
                    self.publisher
                        .publish(&mut tx, events::role_pending(&role, version, event))
                        .await?;
                    tracing::info!(
                        module = policies.module(),
                        role = role.name(),
                        "role created for a default grant"
                    );
                }
                let grant = Grant::restore(GrantId::new(), rule.clone(), GrantedBy::System, now);
                let version = self.repo.insert(&mut tx, &grant).await?;
                self.publisher
                    .publish(
                        &mut tx,
                        events::pending(&grant, version, GrantEvent::Granted),
                    )
                    .await?;
                tracing::info!(
                    module = policies.module(),
                    role = grant.role(),
                    resource = grant.resource(),
                    action = grant.action(),
                    scope = scope_name(grant.scope()),
                    "default grant applied"
                );
                granted += 1;
            }
        }
        // Nothing new leaves the revision where it was, so no instance rebuilds for nothing.
        if recorded > 0 {
            tx.commit().await?;
        }
        Ok(granted)
    }
}
