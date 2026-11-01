//! A module's single registration point (spec §4). Each module core builds one
//! [`Module`] in its `di.rs`; `server` lists them once, in dependency order, and
//! derives the request data, migrations, seeds and worker handlers from that
//! list, so none of them can be registered without the others.

use app_db::{ModuleMigrations, PgTransactions, Pool};
use app_domain::Clock;
use app_error::AppResult;
use app_persistence::{Handlers, Transactions};
use app_web::{Accept, Provide};
use poem::EndpointExt;
use poem::endpoint::BoxEndpoint;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// What every module is built from, made once by the server (or a test driver)
/// and passed to each provider, so they all share one pool and one clock.
#[derive(Clone)]
pub struct Context {
    pool: Pool,
    clock: Arc<dyn Clock>,
}

impl Context {
    pub fn new(pool: Pool, clock: Arc<dyn Clock>) -> Self {
        Self { pool, clock }
    }

    pub fn pool(&self) -> Pool {
        self.pool.clone()
    }

    pub fn clock(&self) -> Arc<dyn Clock> {
        self.clock.clone()
    }

    /// Begins units of work on the pool, stamped by the clock.
    pub fn transactions(&self) -> Arc<dyn Transactions> {
        Arc::new(PgTransactions::new(self.pool(), self.clock()))
    }

    /// Runs `handlers`: the server's `worker` bin runs every module's, a test
    /// driver its own module's.
    pub fn worker(&self, handlers: Handlers) -> app_db::Worker {
        app_db::Worker::new(self.pool(), self.clock(), handlers)
    }
}

type Provider = Box<dyn Fn(&Context, BoxEndpoint<'static>) -> BoxEndpoint<'static> + Send + Sync>;
type HandlersFn = Box<dyn Fn(&Context) -> Handlers + Send + Sync>;
type Seed = Box<dyn Fn(Pool) -> Pin<Box<dyn Future<Output = AppResult<()>> + Send>> + Send + Sync>;

pub struct Module {
    provide: Provider,
    handlers: Option<HandlersFn>,
    migrations: Option<ModuleMigrations>,
    seed: Option<Seed>,
}

impl Module {
    pub fn new<P: Provide + 'static>(
        provider: impl Fn(&Context) -> P + Send + Sync + 'static,
    ) -> Self {
        Self {
            provide: Box::new(move |ctx, app| app.accept(provider(ctx)).boxed()),
            handlers: None,
            migrations: None,
            seed: None,
        }
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

    pub fn with_migrations(self, migrations: ModuleMigrations) -> Self {
        Self {
            migrations: Some(migrations),
            ..self
        }
    }

    /// Call it only under the module's `seed` feature, which prod builds leave off.
    pub fn with_seed<F, Fut>(self, seed: F) -> Self
    where
        F: Fn(Pool) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        Self {
            seed: Some(Box::new(move |pool| Box::pin(seed(pool)))),
            ..self
        }
    }

    pub fn migrations(&self) -> Option<ModuleMigrations> {
        self.migrations
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
    pub async fn seed(&self, pool: Pool) -> AppResult<()> {
        match &self.seed {
            Some(seed) => seed(pool).await,
            None => Ok(()),
        }
    }
}
