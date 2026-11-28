// Naming the pool to wire the repository is this module's DI seam (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::{
    AccessService, AuditEntry, IdentityDirectory, Modules, Operation, Rules, SignIns,
};
use crate::cognito::CognitoDirectory;
use crate::db::{
    AUDIT_ASSIGNMENTS, AUDIT_GRANTS, AUDIT_ROLES, AuditRows, PgAssignmentRepository, PgAuditLog,
    PgGrantRepository, PgInvitationRepository, PgRoleRepository, PgSubjectRepository, migrations,
};
use crate::domain::{Assignment, Grant, Invitation, Role, Subject};
use crate::web::{OPERATIONS, RecordingSignIns};
use access_api::RoleAssignments;
use access_api::events::v1::{AssignmentTopic, GrantTopic, InvitationTopic, RoleTopic};
use app_authz::{Policies, PoliciesBuilder, policy_file};
use app_error::AppResult;
use app_module::{Context, Module, PoliciesError};
use app_persistence::Handlers;
use app_web::{PrincipalResolver, Provide};
use std::sync::Arc;

pub struct AccessProvider(AccessService);

impl Provide for AccessProvider {
    type Data = AccessService;

    fn build(self) -> AccessService {
        self.0
    }
}

impl Modules {
    /// Every module's policies and `OPERATIONS`, `access`' own included.
    pub fn of(modules: &[Module]) -> Result<Self, PoliciesError> {
        Ok(Self {
            policies: modules
                .iter()
                .filter_map(Module::policies)
                .collect::<Result<_, _>>()?,
            operations: modules
                .iter()
                .flat_map(|m| m.operations())
                .map(|p| Operation {
                    id: p.operation,
                    resource: p.resource,
                    action: p.action,
                })
                .collect(),
        })
    }
}

/// The identity provider invitations go through: the Cognito user pool `COGNITO_USER_POOL_ID`
/// names, reached at `COGNITO_ENDPOINT` when set (a stand-in's, as `make e2e`'s); `None` when it
/// is unset or empty, and inviting is off. Read from the environment, as the AWS SDK reads its
/// credentials and region, so an app's config needs nothing for it.
pub fn directory() -> Result<Option<Arc<dyn IdentityDirectory>>, String> {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let Some(pool_id) = var("COGNITO_USER_POOL_ID") else {
        return Ok(None);
    };
    let directory = CognitoDirectory::new(pool_id.trim(), var("COGNITO_ENDPOINT").as_deref())?;
    Ok(Some(Arc::new(directory)))
}

fn service(
    ctx: &Context,
    modules: Arc<Modules>,
    directory: Option<Arc<dyn IdentityDirectory>>,
) -> AccessService {
    AccessService::new(
        Arc::new(PgGrantRepository::new(ctx.pool())),
        Arc::new(PgRoleRepository::new(ctx.pool())),
        Arc::new(PgAssignmentRepository::new(ctx.pool())),
        Arc::new(PgSubjectRepository::new(ctx.pool())),
        Arc::new(PgInvitationRepository::new(ctx.pool())),
        directory,
        ctx.publisher(),
        ctx.transactions(),
        ctx.clock(),
        ctx.authorizer(),
        modules,
        Arc::new(PgAuditLog::new(ctx.pool())),
    )
}

/// `directory` is `None` when no identity provider is configured, and a Fake in tests.
pub fn provider(
    ctx: &Context,
    modules: Arc<Modules>,
    directory: Option<Arc<dyn IdentityDirectory>>,
) -> AccessProvider {
    AccessProvider(service(ctx, modules, directory))
}

/// Resolves every token's Principal from the rule set in force, as the `Authorizer` does, and
/// records its subject as seen signing in: what `module(..)` gives every request.
pub fn sign_ins(ctx: &Context) -> Arc<dyn PrincipalResolver> {
    Arc::new(RecordingSignIns {
        sign_ins: SignIns::new(
            Arc::new(PgSubjectRepository::new(ctx.pool())),
            Arc::new(PgInvitationRepository::new(ctx.pool())),
            ctx.publisher(),
            ctx.transactions(),
            ctx.clock(),
        ),
        authorizer: ctx.authorizer(),
    })
}

/// `access`' `grant`, `role`, `assignment`, `subject`, `invitation` and `auditEntry` resources, all `admin`'s by baseline
/// rules rather than grants, so no revocation can lock admins out (ADR 0040). It has no default
/// grants.
pub fn policies() -> PoliciesBuilder {
    Policies::builder("access")
        .capability(policy_file!("../capability.yaml"))
        .resource::<Grant>(policy_file!("policies/grant.yaml"))
        .resource::<Role>(policy_file!("policies/role.yaml"))
        .resource::<Assignment>(policy_file!("policies/assignment.yaml"))
        .resource::<Subject>(policy_file!("policies/subject.yaml"))
        .resource::<Invitation>(policy_file!("policies/invitation.yaml"))
        .resource::<AuditEntry>(policy_file!("policies/audit_entry.yaml"))
        .labels(policy_file!("policies/labels.yaml"))
}

/// What the worker runs for `access`: the audit log's copy of every grant, role and assignment
/// event, from the earliest each topic holds.
pub fn handlers(_ctx: &Context) -> Handlers {
    Handlers::new()
        .on_from_earliest::<GrantTopic>(AUDIT_GRANTS, AuditRows)
        .on_from_earliest::<RoleTopic>(AUDIT_ROLES, AuditRows)
        .on_from_earliest::<AssignmentTopic>(AUDIT_ASSIGNMENTS, AuditRows)
}

/// Assigning roles, for another module's seed to give the accounts it seeds theirs, as `System`:
/// that module's `module(..)` takes this constructor.
pub fn assignments(ctx: &Context) -> Arc<dyn RoleAssignments> {
    // Assigning reads nothing of what modules can be granted.
    let modules = Modules {
        policies: Vec::new(),
        operations: Vec::new(),
    };
    Arc::new(service(ctx, Arc::new(modules), None))
}

/// What keeps `ctx.authorizer()` in step with the stored grants and assignments: a server
/// instance refreshes it once before serving, then watches it.
pub fn rules(ctx: &Context, modules: Arc<Modules>) -> Rules {
    Rules::new(
        Arc::new(PgGrantRepository::new(ctx.pool())),
        Arc::new(PgAssignmentRepository::new(ctx.pool())),
        modules,
        ctx.authorizer(),
    )
}

/// What `migrate` runs once every module is migrated: retires the Modules' gates if it hasn't
/// yet (ADR 0047), then grants each module's default grants not applied before. Returns how many
/// it granted.
pub async fn apply_default_grants(ctx: &Context, modules: Arc<Modules>) -> AppResult<usize> {
    let service = service(ctx, modules, None);
    let revoked = service.retire_gates().await?;
    if revoked > 0 {
        tracing::info!(revoked, "Modules' gates retired");
    }
    service.apply_defaults().await
}

/// `composed` is every module the server registers, this one included, as the server lists
/// them: what can be granted is what their policies have. Invitations go through
/// [`directory`].
pub fn module(composed: fn() -> Vec<Module>) -> Module {
    Module::new(move |ctx: &Context| {
        let modules = Modules::of(&composed())
            .expect("every module's policies load: the server checks them before it serves");
        let directory = directory().unwrap_or_else(|err| {
            // A malformed id is a deploy mistake: refuse to invite, not to serve.
            tracing::error!(error = %err, "inviting is off: COGNITO_USER_POOL_ID is malformed");
            None
        });
        provider(ctx, Arc::new(modules), directory)
    })
    .providing(sign_ins)
    .with_policies(policies)
    .performing(OPERATIONS)
    .with_handlers(handlers)
    .publishes::<GrantTopic>()
    .publishes::<RoleTopic>()
    .publishes::<AssignmentTopic>()
    .publishes::<InvitationTopic>()
    .with_migrations(migrations())
}
