//! A module's single registration point (ADR 0019). Each module core builds one
//! [`Module`] in its `di.rs`; `server` lists them once, in dependency order, and
//! derives the request data, migrations, seeds and worker handlers from that
//! list, so none of them can be registered without the others.

use app_authz::{Authorizer, Policies, PoliciesBuilder, PolicyError, RuleSet};
use app_bus::{Bus, NoBus};
use app_db::{ModuleMigrations, PgEventPublisher, PgIdempotencyRecords, PgTransactions, Pool};
use app_domain::Clock;
use app_error::AppResult;
use app_events::{PublishedTopic, Topic};
use app_persistence::{EventPublisher, Handlers, Transactions};
use app_web::{Accept, Idempotency, Performs, Provide};
use poem::EndpointExt;
use poem::endpoint::BoxEndpoint;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// What every module is built from, made once by the server (or a test driver)
/// and passed to each provider, so they all share one pool, clock, bus and
/// rule set.
#[derive(Clone)]
pub struct Context {
    pool: Pool,
    clock: Arc<dyn Clock>,
    bus: Arc<dyn Bus>,
    authorizer: Arc<Authorizer>,
}

impl Context {
    /// Its authorizer allows no user anything until [`with_authorizer`](Self::with_authorizer).
    pub fn new(pool: Pool, clock: Arc<dyn Clock>) -> Self {
        Self {
            pool,
            clock,
            bus: Arc::new(NoBus),
            authorizer: Arc::new(Authorizer::new(RuleSet::default())),
        }
    }

    /// The rule set every service plans its actions with and every `Caller`
    /// passes its gate by (ADR 0040), compiled from the modules' policies.
    pub fn with_authorizer(self, authorizer: Arc<Authorizer>) -> Self {
        Self { authorizer, ..self }
    }

    pub fn authorizer(&self) -> Arc<Authorizer> {
        self.authorizer.clone()
    }

    /// What server instances and workers broadcast to each other over (ADR
    /// 0039), shared by every module. Without one (a migration, a test that
    /// opens no stream, an app no module of which broadcasts) nothing is
    /// broadcast.
    pub fn with_bus(self, bus: Arc<dyn Bus>) -> Self {
        Self { bus, ..self }
    }

    pub fn bus(&self) -> Arc<dyn Bus> {
        self.bus.clone()
    }

    pub fn pool(&self) -> Pool {
        self.pool.clone()
    }

    pub fn clock(&self) -> Arc<dyn Clock> {
        self.clock.clone()
    }

    pub fn transactions(&self) -> Arc<dyn Transactions> {
        Arc::new(PgTransactions::new(self.pool()))
    }

    /// Publishes into a unit of work begun by [`transactions`](Self::transactions),
    /// stamped by the clock.
    pub fn publisher(&self) -> Arc<dyn EventPublisher> {
        Arc::new(PgEventPublisher::new(self.clock()))
    }

    /// The `Idempotency-Key` middleware, keeping its records in the pool and
    /// timing them by the clock.
    pub fn idempotency(&self) -> Idempotency {
        Idempotency::new(
            Arc::new(PgIdempotencyRecords::new(self.pool())),
            self.clock(),
        )
    }

    /// Runs `handlers`: the server's `worker` bin runs every module's, a test
    /// driver its own module's.
    pub fn worker(&self, handlers: Handlers) -> app_db::Worker {
        app_db::Worker::new(self.pool(), self.clock(), handlers)
    }
}

type Provider = Box<dyn Fn(&Context, BoxEndpoint<'static>) -> BoxEndpoint<'static> + Send + Sync>;
type HandlersFn = Box<dyn Fn(&Context) -> Handlers + Send + Sync>;
type PoliciesFn = Box<dyn Fn() -> PoliciesBuilder + Send + Sync>;
type Seed =
    Box<dyn Fn(Context) -> Pin<Box<dyn Future<Output = AppResult<()>> + Send>> + Send + Sync>;

pub struct Module {
    provide: Provider,
    handlers: Option<HandlersFn>,
    topics: Vec<PublishedTopic>,
    migrations: Option<ModuleMigrations>,
    seed: Option<Seed>,
    broadcasts: bool,
    policies: Option<PoliciesFn>,
    operations: &'static [Performs],
}

/// A module's policy files that don't load: the server, worker and `migrate`
/// refuse to start on one.
#[derive(Debug, thiserror::Error)]
#[error("the `{module}` module's policies don't load: {source}")]
pub struct PoliciesError {
    pub module: String,
    #[source]
    pub source: PolicyError,
}

impl Module {
    pub fn new<P: Provide + 'static>(
        provider: impl Fn(&Context) -> P + Send + Sync + 'static,
    ) -> Self {
        Self {
            provide: Box::new(move |ctx, app| app.accept(provider(ctx)).boxed()),
            handlers: None,
            topics: Vec::new(),
            migrations: None,
            seed: None,
            broadcasts: false,
            policies: None,
            operations: &[],
        }
    }

    /// The module's gate, resources, scopes and default grants (ADR 0040),
    /// built when the server, worker or `migrate` starts.
    pub fn with_policies(
        self,
        policies: impl Fn() -> PoliciesBuilder + Send + Sync + 'static,
    ) -> Self {
        Self {
            policies: Some(Box::new(policies)),
            ..self
        }
    }

    /// The action each of its guarded operations performs: its web layer's `OPERATIONS`.
    pub fn performing(self, operations: &'static [Performs]) -> Self {
        Self { operations, ..self }
    }

    /// `None` for a module with no policies, which may have no guarded operations.
    pub fn policies(&self) -> Option<Result<Policies, PoliciesError>> {
        let builder = (self.policies.as_ref()?)();
        let module = builder.module().to_owned();
        Some(
            builder
                .build()
                .map_err(|source| PoliciesError { module, source }),
        )
    }

    pub fn operations(&self) -> &'static [Performs] {
        self.operations
    }

    /// The module's event subscriptions and jobs, for the worker.
    pub fn with_handlers(
        self,
        handlers: impl Fn(&Context) -> Handlers + Send + Sync + 'static,
    ) -> Self {
        Self {
            handlers: Some(Box::new(handlers)),
            ..self
        }
    }

    /// A topic the module publishes, which `migrate` registers with its
    /// retention, so the worker compacts it.
    pub fn publishes<T: Topic>(mut self) -> Self {
        self.topics.push(PublishedTopic::of::<T>());
        self
    }

    /// The module publishes or follows channels on the app's bus, so the
    /// server and worker refuse to start without one (ADR 0039).
    pub fn broadcasts(self) -> Self {
        Self {
            broadcasts: true,
            ..self
        }
    }

    pub fn with_migrations(self, migrations: ModuleMigrations) -> Self {
        Self {
            migrations: Some(migrations),
            ..self
        }
    }

    /// Call it only under the module's `seed` feature, which prod builds leave off. It is given
    /// the context, so it can build what it calls in another module's API (`access`, to assign
    /// its seeded users' roles).
    pub fn with_seed<F, Fut>(self, seed: F) -> Self
    where
        F: Fn(Context) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        Self {
            seed: Some(Box::new(move |ctx| Box::pin(seed(ctx)))),
            ..self
        }
    }

    pub fn migrations(&self) -> Option<ModuleMigrations> {
        self.migrations
    }

    pub fn uses_bus(&self) -> bool {
        self.broadcasts
    }

    pub fn topics(&self) -> &[PublishedTopic] {
        &self.topics
    }

    /// Adds the module's request data (its service) to `app`.
    pub fn provide(&self, ctx: &Context, app: BoxEndpoint<'static>) -> BoxEndpoint<'static> {
        (self.provide)(ctx, app)
    }

    /// Empty for a module that neither subscribes nor runs jobs.
    pub fn handlers(&self, ctx: &Context) -> Handlers {
        self.handlers
            .as_ref()
            .map_or_else(Handlers::new, |handlers| handlers(ctx))
    }

    /// A no-op for a module built without its seed.
    pub async fn seed(&self, ctx: &Context) -> AppResult<()> {
        match &self.seed {
            Some(seed) => seed(ctx.clone()).await,
            None => Ok(()),
        }
    }
}
