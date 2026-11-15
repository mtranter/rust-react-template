// The composition root is where the pool meets the repository (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::WidgetService;
use crate::db::{PgWidgetRepository, PgWidgetSearch, SEARCH, SearchRows, migrations};
use app_db::Projecting;
use app_module::{Context, Module};
use app_persistence::Handlers;
use app_web::Provide;
use std::sync::Arc;
use widgets_api::events::v1::WidgetTopic;

pub struct WidgetsProvider(WidgetService);

pub fn provider(ctx: &Context) -> WidgetsProvider {
    WidgetsProvider(WidgetService::new(
        Arc::new(PgWidgetRepository::new(ctx.pool())),
        Arc::new(PgWidgetSearch::new(ctx.pool())),
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

/// What the worker runs for widgets: the projection keeping its search copy,
/// from the earliest its own topic holds.
pub fn handlers(_ctx: &Context) -> Handlers {
    Handlers::new().project::<WidgetTopic>(SEARCH, SearchRows)
}

pub fn module() -> Module {
    let module = Module::new(provider)
        .publishes::<WidgetTopic>()
        .with_handlers(handlers)
        .with_migrations(migrations());
    #[cfg(feature = "seed")]
    let module = module.with_seed(|pool| async move { crate::seed::standard(&pool).await });
    module
}
