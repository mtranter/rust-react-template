// Naming the pool to wire the finder is this module's DI seam (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::WidgetIndexService;
use crate::db::{COPY, EntryRows, PgEntryFinder, migrations};
use app_db::Projecting;
use app_module::{Context, Module};
use app_persistence::Handlers;
use app_web::Provide;
use sqlx::PgPool;
use std::sync::Arc;
use widgets_api::events::v1::WidgetTopic;

pub struct WidgetIndexProvider(PgPool);

pub fn provider(ctx: &Context) -> WidgetIndexProvider {
    WidgetIndexProvider(ctx.pool())
}

impl Provide for WidgetIndexProvider {
    type Data = WidgetIndexService;

    fn build(self) -> WidgetIndexService {
        WidgetIndexService::new(Arc::new(PgEntryFinder::new(self.0)))
    }
}

/// What the worker runs for this module: the projection keeping its copy,
/// from the earliest its topic holds.
pub fn handlers(_ctx: &Context) -> Handlers {
    Handlers::new().project::<WidgetTopic>(COPY, EntryRows)
}

/// No seed: everything it holds is a copy, filled from the provider's events.
pub fn module() -> Module {
    Module::new(provider)
        .with_handlers(handlers)
        .with_migrations(migrations::migrations())
}
