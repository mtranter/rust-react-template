//! Shared by the server, `worker`, `dump-spec`, `migrate`, `seed` and the
//! composed-app tests, so each sees the same modules and the dumped spec is
//! byte-for-byte the one served. Each function takes the [`Composition`] it
//! serves: [`Registered`] everywhere but tests composing modules of their own.

mod app;
pub mod config;
mod exempt;
mod modules;
mod shutdown;
pub mod sign_in;
pub mod telemetry;
mod tools;

pub use app::build_app;
pub use exempt::{PUBLIC, SIGNED_IN};
pub use modules::Registered;
pub use shutdown::shutdown_signal;
pub use tools::Tools;

use app_authz::{Authorizer, Policies};
use app_db::{ModuleMigrations, ScheduleError, SubscriptionError, Worker};
use app_error::AppResult;
use app_module::{Context, Module, PoliciesError};
use app_persistence::Handlers;
use app_web::{Idempotent, Performs};
use poem_openapi::{OpenApi, OpenApiService};
use std::sync::Arc;

/// Modules served as one app: the API's operations and the modules behind them.
pub trait Composition: Send + Sync + 'static {
    /// One type for both the spec and its value (`Endpoints::default()`).
    type Endpoints: OpenApi + Default + Send + Sync + 'static;

    /// In dependency order: migrations and seeds run in it, so a module comes
    /// after the modules whose data or published views it uses.
    fn modules() -> Vec<Module>;
}

/// The app's name, `make init`'s `NAME`: its traces' `service.namespace`.
pub const APP_NAME: &str = "rust-vite-template";
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

/// Every module's baseline rules and no grant (ADR 0040), for the context every
/// service plans with: the server fills it from the stored grants
/// ([`rules`]) before it serves, and the worker, acting as `System`, needs no more.
pub fn authorizer<C: Composition>() -> Result<Authorizer, Box<dyn std::error::Error>> {
    Ok(Authorizer::baseline(&policies::<C>()?)?)
}

/// Every resource and action the modules' policies declare, `(resource, action)`.
pub fn actions<C: Composition>() -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let rule_set = authorizer::<C>()?.current();
    Ok(policies::<C>()?
        .iter()
        .flat_map(|policies| policies.catalog(&rule_set).resources)
        .flat_map(|resource| {
            resource
                .actions
                .into_iter()
                .map(move |action| (resource.kind.clone(), action.name))
        })
        .collect())
}

/// Every module's policies and operations: what `access` grants from.
pub fn access_modules<C: Composition>() -> Result<Arc<access::Modules>, PoliciesError> {
    Ok(Arc::new(access::Modules::of(&C::modules())?))
}

/// What keeps `ctx.authorizer()` in step with the grants and assignments `access`
/// stores: refresh it once before serving, then watch it.
pub fn rules<C: Composition>(ctx: &Context) -> Result<access::Rules, PoliciesError> {
    Ok(access::rules(ctx, access_modules::<C>()?))
}

/// Grants every module's default grants `migrate` hasn't applied before, as
/// `migrate` does once every module is migrated. Returns how many it granted.
pub async fn apply_default_grants<C: Composition>(
    ctx: &Context,
) -> Result<usize, Box<dyn std::error::Error>> {
    Ok(access::apply_default_grants(ctx, access_modules::<C>()?).await?)
}

/// Every Connection the modules declare (`Module::declaring`, ADR 0050).
pub fn declared_connections<C: Composition>() -> Vec<connections_api::Declaration> {
    C::modules()
        .iter()
        .flat_map(|module| module.declared::<connections_api::Declaration>())
        .collect()
}

/// The values `seed` gives each declared Connection in dev (`connections_api::DevConnection`,
/// declared under a module's `seed` feature), pointing it at its fake.
pub fn dev_connections<C: Composition>() -> Vec<connections_api::DevConnection> {
    C::modules()
        .iter()
        .flat_map(|module| module.declared::<connections_api::DevConnection>())
        .collect()
}

/// Records every Connection the modules declare and retires those none does any more, as
/// `migrate` does once every module is migrated. Returns how many changed.
pub async fn declare_connections<C: Composition>(ctx: &Context) -> AppResult<usize> {
    connections::declare(ctx, declared_connections::<C>()).await
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

/// Fails on the first module schedule whose expression doesn't parse, naming
/// it: `migrate` and `worker` refuse to start on one, before changing anything.
pub fn check_schedules<C: Composition>(ctx: &Context) -> Result<(), ScheduleError> {
    app_db::parse_schedules(&handlers::<C>(ctx)).map(|_| ())
}

/// Registers every module's schedules, as `migrate` does after migrating.
pub async fn register_schedules<C: Composition>(ctx: &Context) -> Result<(), ScheduleError> {
    app_db::register_schedules(&ctx.pool(), ctx.clock().as_ref(), &handlers::<C>(ctx)).await
}
