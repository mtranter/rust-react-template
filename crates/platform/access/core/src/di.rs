// Naming the pool to wire the repository is this module's DI seam (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::{AccessService, Modules, Operation, Rules};
use crate::db::{PgAssignmentRepository, PgGrantRepository, PgRoleRepository, migrations};
use crate::domain::{Assignment, Grant, Role};
use crate::web::OPERATIONS;
use access_api::RoleAssignments;
use access_api::events::v1::{AssignmentTopic, GrantTopic, RoleTopic};
use app_authz::{Policies, PoliciesBuilder, policy_file};
use app_error::AppResult;
use app_module::{Context, Module, PoliciesError};
use app_web::Provide;
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

/// `access`' gate and its `grant`, `role` and `assignment` resources, all `admin`'s by baseline
/// rules rather than grants, so no revocation can lock admins out (ADR 0040). It has no default
/// grants.
pub fn policies() -> PoliciesBuilder {
    Policies::builder("access")
        .gate(policy_file!("policies/gate.yaml"))
        .resource::<Grant>(policy_file!("policies/grant.yaml"))
        .resource::<Role>(policy_file!("policies/role.yaml"))
        .resource::<Assignment>(policy_file!("policies/assignment.yaml"))
}

/// Assigning roles, for another module's seed to give its seeded users theirs, as `System`
/// (`users::module(access::assignments)`).
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
    .with_policies(policies)
    .performing(OPERATIONS)
    .publishes::<GrantTopic>()
    .publishes::<RoleTopic>()
    .publishes::<AssignmentTopic>()
    .with_migrations(migrations())
}
