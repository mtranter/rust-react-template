use crate::support::{Messaging, Pinged};
use app_db::conn;
use app_error::{AppError, AppResult};
use app_events::EnvelopeOf;
use app_persistence::{Handle, Handlers, Tx};
use async_trait::async_trait;
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
