//! Shared by the server, `worker`, `dump-spec`, `migrate`, `seed` and the
//! composed-app tests, so each sees the same modules and the dumped spec is
//! byte-for-byte the one served. Each function takes the [`Composition`] it
//! serves: [`Registered`] everywhere but tests composing modules of their own.

mod app;
pub mod config;
mod modules;
mod public;
mod shutdown;
pub mod telemetry;
mod tools;

pub use app::build_app;
pub use modules::Registered;
pub use public::PUBLIC;
pub use shutdown::shutdown_signal;
pub use tools::Tools;

use app_authz::{Authorizer, Policies};
use app_db::{ModuleMigrations, SubscriptionError, Worker};
use app_error::AppResult;
use app_module::{Context, Module, PoliciesError};
use app_persistence::Handlers;
use app_web::{Idempotent, Performs};
use poem_openapi::{OpenApi, OpenApiService};

/// Modules served as one app: the API's operations and the modules behind them.
pub trait Composition: Send + Sync + 'static {
    /// One type for both the spec and its value (`Endpoints::default()`).
    type Endpoints: OpenApi + Default + Send + Sync + 'static;

    /// In dependency order: migrations and seeds run in it, so a module comes
    /// after the modules whose data or published views it uses.
    fn modules() -> Vec<Module>;
}

pub const API_TITLE: &str = "rust-vite-template API";
pub const API_VERSION: &str = "0.1.0";
/// Also the generated client's `baseUrl`.
pub const API_BASE: &str = "/api";

pub fn build_service<C: Composition>() -> OpenApiService<Idempotent<C::Endpoints>, ()> {
    OpenApiService::new(
        Idempotent::<C::Endpoints>::default(),
        API_TITLE,
        API_VERSION,
    )
    .server(API_BASE)
}

/// The spec as `dump-spec` writes it and `/api/spec.json` serves it.
pub fn spec<C: Composition>() -> String {
    app_web::with_error_shape(build_service::<C>().spec())
}

/// Whether any module broadcasts, so the server and worker need the bus.
pub fn uses_bus<C: Composition>() -> bool {
    C::modules().iter().any(|module| module.uses_bus())
}

/// Every module's policies, built as the server, worker and `migrate` start:
/// the first that doesn't load is the error, naming its module, file and line.
pub fn policies<C: Composition>() -> Result<Vec<Policies>, PoliciesError> {
    C::modules().iter().filter_map(Module::policies).collect()
}

/// One rule set from every module's policies and default grants (ADR 0040),
/// for the context every service and `Caller` plans with.
pub fn authorizer<C: Composition>() -> Result<Authorizer, Box<dyn std::error::Error>> {
    Ok(Authorizer::from_policies(&policies::<C>()?)?)
}

/// Every module's `OPERATIONS`: the action each guarded operation performs.
pub fn operations<C: Composition>() -> Vec<Performs> {
    C::modules()
        .iter()
        .flat_map(|module| module.operations().iter().copied())
        .collect()
}

pub fn module_migrations<C: Composition>() -> Vec<ModuleMigrations> {
    C::modules().iter().filter_map(|m| m.migrations()).collect()
}

/// Every module's subscriptions. Panics if two modules claim one queue name.
pub fn handlers<C: Composition>(ctx: &Context) -> Handlers {
    C::modules().iter().fold(Handlers::new(), |all, module| {
        all.merge(module.handlers(ctx))
    })
}

/// Runs every module's subscriptions.
pub fn worker<C: Composition>(ctx: &Context) -> Worker {
    ctx.worker(handlers::<C>(ctx))
}

/// Registers every module's topics, as `migrate` does after migrating, so
/// every worker compacts each by its retention.
pub async fn register_topics<C: Composition>(ctx: &Context) -> AppResult<()> {
    let topics: Vec<_> = C::modules()
        .iter()
        .flat_map(|module| module.topics().to_vec())
        .collect();
    app_db::register_topics(&ctx.pool(), &topics).await
}

/// Registers every module's subscriptions, as `migrate` does after migrating.
pub async fn register_subscriptions<C: Composition>(
    ctx: &Context,
) -> Result<(), SubscriptionError> {
    app_db::register_subscriptions(&ctx.pool(), ctx.clock().as_ref(), &handlers::<C>(ctx)).await
}
