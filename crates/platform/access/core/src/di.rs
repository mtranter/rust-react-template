// Naming the pool to wire the repository is this module's DI seam (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::{AccessService, Modules, Operation, Rules, SignIns};
use crate::db::{
    PgAssignmentRepository, PgGrantRepository, PgRoleRepository, PgSubjectRepository, migrations,
};
use crate::domain::{Assignment, Grant, Role, Subject};
use crate::web::{OPERATIONS, RecordingSignIns};
use access_api::RoleAssignments;
use access_api::events::v1::{AssignmentTopic, GrantTopic, RoleTopic};
use app_authz::{Policies, PoliciesBuilder, policy_file};
use app_error::AppResult;
use app_module::{Context, Module, PoliciesError};
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

fn service(ctx: &Context, modules: Arc<Modules>) -> AccessService {
    AccessService::new(
        Arc::new(PgGrantRepository::new(ctx.pool())),
        Arc::new(PgRoleRepository::new(ctx.pool())),
        Arc::new(PgAssignmentRepository::new(ctx.pool())),
        Arc::new(PgSubjectRepository::new(ctx.pool())),
        ctx.publisher(),
        ctx.transactions(),
        ctx.clock(),
        ctx.authorizer(),
        modules,
    )
}

pub fn provider(ctx: &Context, modules: Arc<Modules>) -> AccessProvider {
    AccessProvider(service(ctx, modules))
}

/// Resolves every token's Principal from the rule set in force, as the `Authorizer` does, and
/// records its subject as seen signing in: what `module(..)` gives every request.
pub fn sign_ins(ctx: &Context) -> Arc<dyn PrincipalResolver> {
    Arc::new(RecordingSignIns {
        sign_ins: SignIns::new(Arc::new(PgSubjectRepository::new(ctx.pool())), ctx.clock()),
        authorizer: ctx.authorizer(),
    })
}

/// `access`' gate and its `grant`, `role`, `assignment` and `subject` resources, all `admin`'s by baseline
/// rules rather than grants, so no revocation can lock admins out (ADR 0040). It has no default
/// grants.
pub fn policies() -> PoliciesBuilder {
    Policies::builder("access")
        .gate(policy_file!("policies/gate.yaml"))
        .resource::<Grant>(policy_file!("policies/grant.yaml"))
        .resource::<Role>(policy_file!("policies/role.yaml"))
        .resource::<Assignment>(policy_file!("policies/assignment.yaml"))
        .resource::<Subject>(policy_file!("policies/subject.yaml"))
        .labels(policy_file!("policies/labels.yaml"))
}

/// Assigning roles, for another module's seed to give the accounts it seeds theirs, as `System`:
/// that module's `module(..)` takes this constructor.
pub fn assignments(ctx: &Context) -> Arc<dyn RoleAssignments> {
    // Assigning reads nothing of what modules can be granted.
    let modules = Modules {
        policies: Vec::new(),
        operations: Vec::new(),
    };
    Arc::new(service(ctx, Arc::new(modules)))
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

/// What `migrate` runs once every module is migrated: grants each module's default grants not
/// applied before. Returns how many it granted.
pub async fn apply_default_grants(ctx: &Context, modules: Arc<Modules>) -> AppResult<usize> {
    service(ctx, modules).apply_defaults().await
}

/// `composed` is every module the server registers, this one included, as the server lists
/// them: what can be granted is what their policies have.
pub fn module(composed: fn() -> Vec<Module>) -> Module {
    Module::new(move |ctx: &Context| {
        let modules = Modules::of(&composed())
            .expect("every module's policies load: the server checks them before it serves");
        provider(ctx, Arc::new(modules))
    })
    .providing(sign_ins)
    .with_policies(policies)
    .performing(OPERATIONS)
    .publishes::<GrantTopic>()
    .publishes::<RoleTopic>()
    .publishes::<AssignmentTopic>()
    .with_migrations(migrations())
}
