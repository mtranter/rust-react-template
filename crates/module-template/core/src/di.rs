// The composition root is where the pool meets the repository (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::WidgetService;
use crate::db::{PgWidgetRepository, PgWidgetSearch, SEARCH, SearchRows, migrations};
use crate::domain::Widget;
use crate::web::OPERATIONS;
use app_authz::{Policies, PoliciesBuilder, policy_file};
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
        ctx.authorizer(),
    ))
}

/// The widgets module's gate and its `widget` resource, and what `user` and
/// `admin` are granted by default (ADR 0040). Add a scope (a derived role
/// whose `parentRoles` is `["*"]`) to grant an action on some widgets only.
pub fn policies() -> PoliciesBuilder {
    Policies::builder("widgets")
        .resource::<Widget>(policy_file!("policies/widget.yaml"))
        .default_grants(policy_file!("policies/default_grants.yaml"))
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
        .with_policies(policies)
        .performing(OPERATIONS)
        .publishes::<WidgetTopic>()
        .with_handlers(handlers)
        .with_migrations(migrations());
    #[cfg(feature = "seed")]
    let module = module.with_seed(|pool| async move { crate::seed::standard(&pool).await });
    module
}
