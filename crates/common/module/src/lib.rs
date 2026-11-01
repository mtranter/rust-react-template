//! A module's single registration point (spec §4). Each module core builds one
//! [`Module`] in its `di.rs`; `server` lists them once, in dependency order, and
//! derives the request data, migrations and seeds from that list, so none of
//! them can be registered without the others.

use app_db::{ModuleMigrations, Pool};
use app_error::AppResult;
use app_web::{Accept, Provide};
use poem::EndpointExt;
use poem::endpoint::BoxEndpoint;
use std::future::Future;
use std::pin::Pin;

type Provider = Box<dyn Fn(Pool, BoxEndpoint<'static>) -> BoxEndpoint<'static> + Send + Sync>;
type Seed = Box<dyn Fn(Pool) -> Pin<Box<dyn Future<Output = AppResult<()>> + Send>> + Send + Sync>;

pub struct Module {
    provide: Provider,
    migrations: Option<ModuleMigrations>,
    seed: Option<Seed>,
}

impl Module {
    pub fn new<P: Provide + 'static>(provider: impl Fn(Pool) -> P + Send + Sync + 'static) -> Self {
        Self {
            provide: Box::new(move |pool, app| app.accept(provider(pool)).boxed()),
            migrations: None,
            seed: None,
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
    pub fn provide(&self, pool: Pool, app: BoxEndpoint<'static>) -> BoxEndpoint<'static> {
        (self.provide)(pool, app)
    }

    /// A no-op for a module built without its seed.
    pub async fn seed(&self, pool: Pool) -> AppResult<()> {
        match &self.seed {
            Some(seed) => seed(pool).await,
            None => Ok(()),
        }
    }
}
