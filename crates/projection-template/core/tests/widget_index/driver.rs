//! The only place in these tests that knows the transport (ADR 0004). The
//! provider runs for real: its migrations, and its seed, which announces each
//! seeded root before this module exists, so the copy starts from the
//! backfill (ADR 0036).

use app_db::Worker;
use app_domain::{FixedClock, seed_time};
use app_events::Topic;
use app_module::Context;
use app_persistence::Handlers;
use app_test_db::Template;
use app_web::testing::{Page, Refused, TestApi};
use app_web::{Accept, json_newtype};
use sqlx::PgPool;
use std::sync::Arc;
use widget_index::WidgetIndexEndpoints;
use widgets_api::events::v1::WidgetTopic;

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The copy hasn't caught up with what the provider had published yet.
    Unavailable,
}

impl From<Refused> for Refusal {
    fn from(refused: Refused) -> Self {
        match refused.code.as_str() {
            "unavailable" => Refusal::Unavailable,
            other => panic!("unexpected error code {other}"),
        }
    }
}

json_newtype!(pub EntryJson);

pub type Listed = Page<EntryJson>;

pub struct WidgetIndex {
    api: TestApi,
    ctx: Context,
    worker: Worker,
}

impl WidgetIndex {
    /// Registered in an app whose provider had already announced its seeds,
    /// before its worker has run.
    pub async fn added_after_the_seeds(pool: PgPool) -> Self {
        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let ctx = Context::new(pool, clock.clone());
        let handlers = widget_index::handlers(&ctx);
        app_db::register_subscriptions(&ctx.pool(), clock.as_ref(), &handlers)
            .await
            .expect("register the projection's subscription");
        Self {
            api: TestApi::new(WidgetIndexEndpoints, ctx.idempotency(), |app| {
                app.accept(widget_index::provider(&ctx))
            }),
            worker: ctx.worker(handlers),
            ctx,
        }
    }

    /// As [`added_after_the_seeds`](Self::added_after_the_seeds), caught up.
    pub async fn seeded(pool: PgPool) -> Self {
        let index = Self::added_after_the_seeds(pool).await;
        index.run_pending_events().await;
        index
    }

    pub async fn run_pending_events(&self) {
        self.worker
            .run_until_idle()
            .await
            .expect("the worker reaches the database");
    }

    /// The key of every root the provider has announced, sorted.
    pub async fn announced_keys(&self) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT DISTINCT key FROM messaging.outbox WHERE topic = $1 ORDER BY key",
        )
        .bind(<WidgetTopic as Topic>::NAME)
        .fetch_all(&self.ctx.pool())
        .await
        .expect("read the outbox")
    }

    pub async fn all(&self) -> Result<Listed, Refusal> {
        self.api.get("/api/widget-index").send().await
    }
}

/// Every test's database starts as a copy of this: the provider migrated and
/// seeded, its seeds' events relayed before this module was registered.
pub const TEMPLATE: Template = Template::new("widget_index", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &[widgets::migrations(), widget_index::migrations()])
            .await
            .expect("migrate the provider and this module");
        widgets::seed::standard(&pool)
            .await
            .expect("seed the provider");
        Context::new(pool, Arc::new(app_domain::SystemClock))
            .worker(Handlers::new())
            .relay()
            .await
            .expect("relay the seeds' events");
    })
});
