// The composition root is where the pool meets the repository (spec §4).
#![allow(clippy::disallowed_types)]

use crate::application::{Mailer, UserService, Welcome};
use crate::db::{PgUserRepository, migrations};
use crate::mail::LogMailer;
use app_module::{Context, Module};
use app_persistence::Handlers;
use app_web::Provide;
use std::sync::Arc;
use users_api::UserDirectory;
use users_api::events::v1::UserRegistered;

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
    )
}

pub fn directory(ctx: &Context) -> Arc<dyn UserDirectory> {
    Arc::new(service(ctx))
}

/// What the worker runs for users; `mailer` is a Fake in tests.
pub fn handlers(ctx: &Context, mailer: Arc<dyn Mailer>) -> Handlers {
    let repo = Arc::new(PgUserRepository::new(ctx.pool()));
    Handlers::new().on::<UserRegistered>("users.welcome_new_user", Welcome::new(repo, mailer))
}

pub fn module() -> Module {
    let module = Module::new(provider)
        .with_handlers(|ctx| handlers(ctx, Arc::new(LogMailer)))
        .with_migrations(migrations());
    #[cfg(feature = "seed")]
    let module = module.with_seed(|pool| async move { crate::seed::standard(&pool).await });
    module
}
