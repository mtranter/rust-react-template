// The composition root is where the pool meets the repository (spec §4).
#![allow(clippy::disallowed_types)]

use crate::application::WidgetService;
use crate::db::{PgWidgetRepository, migrations};
use app_module::Module;
use app_web::Provide;
use sqlx::PgPool;
use std::sync::Arc;

pub struct WidgetsProvider(PgPool);

pub fn provider(pool: PgPool) -> WidgetsProvider {
    WidgetsProvider(pool)
}

impl Provide for WidgetsProvider {
    type Data = WidgetService;

    fn build(self) -> WidgetService {
        WidgetService::new(Arc::new(PgWidgetRepository::new(self.0)))
    }
}

pub fn module() -> Module {
    let module = Module::new(provider).with_migrations(migrations());
    #[cfg(feature = "seed")]
    let module = module.with_seed(|pool| async move { crate::seed::standard(&pool).await });
    module
}
