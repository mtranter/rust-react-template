// The composition root is where the pool meets the repository (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::{ConnectionService, KeyProvider, Transport};
use crate::db::{PgConnectionRepository, migrations};
use crate::domain::Connection;
use crate::http::HttpTransport;
use crate::master_key::EnvKeyProvider;
use crate::web::OPERATIONS;
use app_authz::{Policies, PoliciesBuilder, policy_file};
use app_error::AppResult;
use app_module::{Context, Module};
use app_web::Provide;
use connections_api::events::v1::ConnectionTopic;
use connections_api::{Clients, Declaration, DevConnection};
use std::sync::Arc;

pub struct ConnectionsProvider(ConnectionService);

impl Provide for ConnectionsProvider {
    type Data = ConnectionService;

    fn build(self) -> ConnectionService {
        self.0
    }
}

/// What seals secrets: `CONNECTIONS_MASTER_KEY` (32 bytes, base64), read from the environment
/// as `access` reads its identity provider's; `None` when it is unset or empty, and secrets
/// can't be set.
pub fn key_provider() -> Result<Option<Arc<dyn KeyProvider>>, String> {
    Ok(EnvKeyProvider::from_env()?.map(|keys| Arc::new(keys) as Arc<dyn KeyProvider>))
}

fn service(
    ctx: &Context,
    keys: Option<Arc<dyn KeyProvider>>,
    transport: Arc<dyn Transport>,
) -> ConnectionService {
    ConnectionService::new(
        Arc::new(PgConnectionRepository::new(ctx.pool())),
        keys,
        ctx.publisher(),
        ctx.transactions(),
        ctx.clock(),
        ctx.authorizer(),
        transport,
    )
}

/// `keys` is `None` when no master key is configured, and any `KeyProvider` in tests.
pub fn provider(ctx: &Context, keys: Option<Arc<dyn KeyProvider>>) -> ConnectionsProvider {
    ConnectionsProvider(service(ctx, keys, Arc::new(HttpTransport::new())))
}

/// As [`provider`], sending through `transport`: a test's, to see what goes on the wire.
pub fn provider_sending(
    ctx: &Context,
    keys: Option<Arc<dyn KeyProvider>>,
    transport: Arc<dyn Transport>,
) -> ConnectionsProvider {
    ConnectionsProvider(service(ctx, keys, transport))
}

/// What other modules call out through (`connections_api::Clients`), passed to their
/// `module(..)` as `connections::clients`. Its OAuth 2 tokens are cached for its life, so build
/// one per process. Seals with [`key_provider`]'s key.
pub fn clients(ctx: &Context) -> Arc<dyn Clients> {
    Arc::new(service(ctx, env_keys(), Arc::new(HttpTransport::new())))
}

/// As [`clients`], opening secrets with `keys`, for tests.
pub fn clients_with(ctx: &Context, keys: Option<Arc<dyn KeyProvider>>) -> Arc<dyn Clients> {
    Arc::new(service(ctx, keys, Arc::new(HttpTransport::new())))
}

fn env_keys() -> Option<Arc<dyn KeyProvider>> {
    key_provider().unwrap_or_else(|err| {
        // A malformed key is a deploy mistake: refuse to set secrets, not to serve.
        tracing::error!(error = %err, "secrets can't be set: CONNECTIONS_MASTER_KEY is malformed");
        None
    })
}

/// The `connection` resource, `admin`'s by default (ADR 0050: no role of its own).
pub fn policies() -> PoliciesBuilder {
    Policies::builder("connections")
        .capability(policy_file!("../capability.yaml"))
        .resource::<Connection>(policy_file!("policies/connection.yaml"))
        .default_grants(policy_file!("policies/default_grants.yaml"))
        .labels(policy_file!("policies/labels.yaml"))
}

/// What `migrate` runs once every module is migrated: records each Connection the modules
/// declare (`Module::declaring`) and retires those none does any more. Returns how many changed.
pub async fn declare(ctx: &Context, declarations: Vec<Declaration>) -> AppResult<usize> {
    service(ctx, None, Arc::new(HttpTransport::new()))
        .declare(declarations)
        .await
}

/// What the `seed` bin runs after `migrate`: points each declared Connection with no URL yet at
/// the fake its module declares for dev ([`DevConnection`]), sealing its secrets with
/// [`key_provider`]'s key. Returns how many it configured.
pub async fn configure_for_dev(ctx: &Context, devs: Vec<DevConnection>) -> AppResult<usize> {
    configure_for_dev_with(ctx, env_keys(), devs).await
}

/// As [`configure_for_dev`], sealing with `keys`, for tests.
pub async fn configure_for_dev_with(
    ctx: &Context,
    keys: Option<Arc<dyn KeyProvider>>,
    devs: Vec<DevConnection>,
) -> AppResult<usize> {
    service(ctx, keys, Arc::new(HttpTransport::new()))
        .configure_for_dev(devs)
        .await
}

/// Seals secrets through [`key_provider`].
pub fn module() -> Module {
    Module::new(|ctx: &Context| provider(ctx, env_keys()))
        .with_policies(policies)
        .performing(OPERATIONS)
        .publishes::<ConnectionTopic>()
        .with_migrations(migrations())
}
