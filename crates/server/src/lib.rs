//! Shared by the server, `worker`, `dump-spec`, `migrate`, `seed` and the
//! composed-app tests, so each sees the same modules and the dumped spec is
//! byte-for-byte the one served.

mod app;
pub mod config;
mod modules;
mod shutdown;
pub mod telemetry;

pub use app::build_app;
pub use modules::{Endpoints, modules};
pub use shutdown::shutdown_signal;

use app_db::{ModuleMigrations, SubscriptionError, Worker};
use app_module::Context;
use app_persistence::Handlers;
use poem_openapi::OpenApiService;

pub const API_TITLE: &str = "rust-vite-template API";
pub const API_VERSION: &str = "0.1.0";
/// Also the generated client's `baseUrl`.
pub const API_BASE: &str = "/api";

pub fn build_service() -> OpenApiService<Endpoints, ()> {
    OpenApiService::new(Endpoints::default(), API_TITLE, API_VERSION).server(API_BASE)
}

pub fn module_migrations() -> Vec<ModuleMigrations> {
    modules().iter().filter_map(|m| m.migrations()).collect()
}

/// Every module's subscriptions. Panics if two modules claim one queue name.
pub fn handlers(ctx: &Context) -> Handlers {
    modules().iter().fold(Handlers::new(), |all, module| {
        all.merge(module.handlers(ctx))
    })
}

/// Runs every module's subscriptions.
pub fn worker(ctx: &Context) -> Worker {
    ctx.worker(handlers(ctx))
}

/// Registers every module's subscriptions, as `migrate` does after migrating.
pub async fn register_subscriptions(ctx: &Context) -> Result<(), SubscriptionError> {
    app_db::register_subscriptions(&ctx.pool(), &handlers(ctx)).await
}
