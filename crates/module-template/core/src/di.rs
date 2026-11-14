// The composition root is where the pool meets the repository (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::WidgetService;
use crate::db::{PgWidgetRepository, migrations};
use app_module::{Context, Module};
use app_web::Provide;
use std::sync::Arc;
use widgets_api::events::v1::WidgetTopic;

pub struct WidgetsProvider(WidgetService);

pub fn provider(ctx: &Context) -> WidgetsProvider {
    WidgetsProvider(WidgetService::new(
        Arc::new(PgWidgetRepository::new(ctx.pool())),
        ctx.publisher(),
        ctx.transactions(),
        ctx.clock(),
    ))
}

impl Provide for WidgetsProvider {
    type Data = WidgetService;

    fn build(self) -> WidgetService {
        self.0
    }
}

pub fn module() -> Module {
    let module = Module::new(provider)
        .publishes::<WidgetTopic>()
        .with_migrations(migrations());
    #[cfg(feature = "seed")]
    let module = module.with_seed(|pool| async move { crate::seed::standard(&pool).await });
    module
}
