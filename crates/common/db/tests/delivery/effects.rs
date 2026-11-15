use crate::support::{Job, Messaging, Pinged};
use app_db::conn;
use app_error::{AppError, AppResult};
use app_events::EnvelopeOf;
use app_persistence::{Handle, Handlers, Tx};
use async_trait::async_trait;
use chrono::TimeDelta;
use sqlx::PgPool;

const QUEUE: &str = "tests.first";

/// Records each ping in `tests_effect`, and while doing so has another worker
/// complete the same delivery, as one would once this run outlived its lease.
struct OutlivesItsLease {
    pool: PgPool,
}

#[async_trait]
impl Handle<Pinged> for OutlivesItsLease {
    async fn handle(&self, tx: &mut Tx, event: EnvelopeOf<Pinged>) -> AppResult<()> {
        sqlx::query("INSERT INTO tests_effect (key) VALUES ($1)")
            .bind(&event.partition_key)
            .execute(conn(tx))
            .await
            .map_err(AppError::internal)?;
        sqlx::query("DELETE FROM messaging.job WHERE queue = $1")
            .bind(QUEUE)
            .execute(&self.pool)
            .await
            .map_err(AppError::internal)?;
        Ok(())
    }
}

/// Has its subscription retired by another connection while it runs, as a
/// migration would during a deploy, then succeeds or fails.
struct RetiredWhileRunning {
    pool: PgPool,
    fails: bool,
}

#[async_trait]
impl Handle<Pinged> for RetiredWhileRunning {
    async fn handle(&self, _: &mut Tx, _: EnvelopeOf<Pinged>) -> AppResult<()> {
        sqlx::query("SELECT messaging.retire($1)")
            .bind(QUEUE)
            .execute(&self.pool)
            .await
            .map_err(AppError::internal)?;
        if self.fails {
            return Err(AppError::conflict("not now"));
        }
        Ok(())
    }
}

async fn retired_while_running(messaging: &Messaging, fails: bool) -> app_db::Worker {
    let handlers = Handlers::new().on::<Pinged>(
        QUEUE,
        RetiredWhileRunning {
            pool: messaging.pool.clone(),
            fails,
        },
    );
    messaging.register(&handlers).await.unwrap();
    let worker = messaging.worker_of(handlers);
    messaging.ping("alice").await;
    worker.relay().await.unwrap();
    worker
}

async fn with_effect_table(pool: PgPool) -> Messaging {
    sqlx::query("CREATE TABLE tests_effect (key text NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    Messaging::new(pool).await
}

async fn effects(messaging: &Messaging) -> Vec<String> {
    sqlx::query_scalar("SELECT key FROM tests_effect ORDER BY key")
        .fetch_all(&messaging.pool)
        .await
        .unwrap()
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_delivery_another_worker_completed_meanwhile_commits_none_of_its_writes(pool: PgPool) {
    let messaging = with_effect_table(pool).await;
    let handlers = Handlers::new().on::<Pinged>(
        QUEUE,
        OutlivesItsLease {
            pool: messaging.pool.clone(),
        },
    );
    messaging.register(&handlers).await.unwrap();
    let worker = messaging.worker_of(handlers);
    messaging.ping("alice").await;
    worker.relay().await.unwrap();

    assert!(worker.run_next().await.unwrap(), "it ran the delivery");

    assert_eq!(effects(&messaging).await, Vec::<String>::new());
    assert_eq!(messaging.jobs().await, []);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_delivery_that_fails_after_its_subscription_was_retired_stays_dead(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let worker = retired_while_running(&messaging, true).await;

    assert!(worker.run_next().await.unwrap(), "it ran the delivery");
    messaging.later(TimeDelta::hours(1));

    assert!(!worker.run_next().await.unwrap(), "nothing runs it again");
    assert_eq!(
        messaging.jobs().await,
        [Job {
            attempts: 1,
            ..Job::retired(QUEUE)
        }]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_delivery_that_succeeds_after_its_subscription_was_retired_is_removed(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let worker = retired_while_running(&messaging, false).await;

    assert!(worker.run_next().await.unwrap(), "it ran the delivery");

    assert_eq!(messaging.jobs().await, []);
}
