// The composition root is where the pool meets the repository (spec §4).
#![allow(clippy::disallowed_types)]

use crate::application::UserService;
use crate::db::{PgUserRepository, migrations};
use app_module::Module;
use app_web::Provide;
use sqlx::PgPool;
use std::sync::Arc;
use users_api::UserDirectory;

pub struct UsersProvider(PgPool);

pub fn provider(pool: PgPool) -> UsersProvider {
    UsersProvider(pool)
}

impl Provide for UsersProvider {
    type Data = UserService;

    fn build(self) -> UserService {
        UserService::new(Arc::new(PgUserRepository::new(self.0)))
    }
}

pub fn directory(pool: PgPool) -> Arc<dyn UserDirectory> {
    Arc::new(UsersProvider(pool).build())
}

pub fn module() -> Module {
    let module = Module::new(provider).with_migrations(migrations());
    #[cfg(feature = "seed")]
    let module = module.with_seed(|pool| async move { crate::seed::standard(&pool).await });
    module
}
