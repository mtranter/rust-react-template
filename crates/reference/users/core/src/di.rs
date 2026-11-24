// The composition root is where the pool meets the repository (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::{Mailer, UserService, Welcome};
use crate::db::{PgUserRepository, migrations};
use crate::domain::User;
use crate::mail::LogMailer;
use crate::web::OPERATIONS;
use access_api::RoleAssignments;
use app_authz::{Policies, PoliciesBuilder, policy_file};
use app_module::{Context, Module};
use app_persistence::Handlers;
use app_web::Provide;
use std::sync::Arc;
use users_api::UserDirectory;
use users_api::events::v1::{UserRegistered, UserTopic};

pub struct UsersProvider(UserService);

pub fn provider(ctx: &Context) -> UsersProvider {
    UsersProvider(service(ctx))
}

impl Provide for UsersProvider {
    type Data = UserService;

    fn build(self) -> UserService {
        self.0
    }
}

fn service(ctx: &Context) -> UserService {
    UserService::new(
        Arc::new(PgUserRepository::new(ctx.pool())),
        ctx.publisher(),
        ctx.transactions(),
        ctx.clock(),
        ctx.authorizer(),
    )
}

/// The users module's `user` resource and the `self` scope, and
/// what `user` and `admin` are granted by default (ADR 0040).
pub fn policies() -> PoliciesBuilder {
    Policies::builder("users")
        .capability(policy_file!("../../capability.yaml"))
        .resource::<User>(policy_file!("policies/user.yaml"))
        .derived_roles(policy_file!("policies/derived_roles.yaml"))
        .default_grants(policy_file!("policies/default_grants.yaml"))
        .labels(policy_file!("policies/labels.yaml"))
}

pub fn directory(ctx: &Context) -> Arc<dyn UserDirectory> {
    Arc::new(service(ctx))
}

/// What the worker runs for users; `mailer` is a Fake in tests.
pub fn handlers(ctx: &Context, mailer: Arc<dyn Mailer>) -> Handlers {
    let repo = Arc::new(PgUserRepository::new(ctx.pool()));
    Handlers::new().on::<UserRegistered>("users.welcome_new_user", Welcome::new(repo, mailer))
}

/// `assignments` is `access`', through which the seed gives its users their roles.
pub fn module(assignments: fn(&Context) -> Arc<dyn RoleAssignments>) -> Module {
    let module = Module::new(provider)
        .with_policies(policies)
        .performing(OPERATIONS)
        .publishes::<UserTopic>()
        .with_handlers(|ctx| handlers(ctx, Arc::new(LogMailer)))
        .with_migrations(migrations());
    #[cfg(feature = "seed")]
    let module = module.with_seed(move |ctx| async move {
        crate::seed::standard(&ctx.pool()).await?;
        crate::seed::assign_roles(assignments(&ctx).as_ref()).await
    });
    #[cfg(not(feature = "seed"))]
    let _ = assignments;
    module
}
