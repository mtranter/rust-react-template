use crate::support::{Messaging, PingEvent, PingTopic, drop, ping, pong, run};
use app_db::{Generations, Project, Projecting, caught_up, conn};
use app_domain::seed_time;
use app_error::{AppError, AppResult};
use app_events::TopicEnvelope;
use app_persistence::{Handlers, PendingEvent, Transactions, Tx};
use async_trait::async_trait;
use chrono::TimeDelta;
use sqlx::PgPool;
use uuid::Uuid;

const COPY: &str = "tests.copy";
const COPY_V2: &str = "tests.copy_v2";

/// Each Ping's latest event, in `tests_copy`: `<key> <event> v<version>`.
struct Rows;

#[async_trait]
impl Project<PingTopic> for Rows {
    fn removes(&self, event: &PingEvent) -> bool {
        matches!(event, PingEvent::Dropped)
    }

    async fn put(&self, tx: &mut Tx, event: &TopicEnvelope<PingTopic>) -> AppResult<()> {
        let what = match event.event {
            PingEvent::Pinged => "pinged",
            PingEvent::Ponged => "ponged",
            PingEvent::Dropped => unreachable!("a removal"),
        };
        sqlx::query(
            "INSERT INTO tests_copy (key, row) VALUES ($1, $2) \
             ON CONFLICT (key) DO UPDATE SET row = EXCLUDED.row",
        )
        .bind(&event.key)
        .bind(format!("{} {what} v{}", event.key, event.version))
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }

    async fn remove(&self, tx: &mut Tx, key: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM tests_copy WHERE key = $1")
            .bind(key)
            .execute(conn(tx))
            .await
            .map_err(AppError::internal)?;
        Ok(())
    }
}

fn projecting(queue: &'static str) -> Handlers {
    Handlers::new().project::<PingTopic>(queue, Rows)
}

async fn with_copy_table(pool: PgPool) -> Messaging {
    sqlx::query("CREATE TABLE tests_copy (key text PRIMARY KEY, row text NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    Messaging::new(pool).await
}

async fn rows(messaging: &Messaging) -> Vec<String> {
    sqlx::query_scalar("SELECT row FROM tests_copy ORDER BY key")
        .fetch_all(&messaging.pool)
        .await
        .unwrap()
}

/// Hands `event` to the projection's handler as one delivery, in its own unit of work.
async fn deliver(messaging: &Messaging, handlers: &Handlers, event: PendingEvent) {
    let payload =
        serde_json::to_value(event.envelope(Uuid::new_v4(), seed_time(0)).unwrap()).unwrap();
    let mut tx = app_db::PgTransactions::new(messaging.pool.clone())
        .begin()
        .await
        .unwrap();
    handlers.handle(COPY, &mut tx, payload).await.unwrap();
    tx.commit().await.unwrap();
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_projection_keeps_each_keys_latest_state_from_the_earliest(pool: PgPool) {
    let messaging = with_copy_table(pool).await;
    messaging.migrate(&["tests.first"]).await;
    messaging
        .publish(vec![ping("alice", 1), ping("bob", 1), pong("alice", 2)])
        .await
        .unwrap();
    run(&messaging.worker(&["tests.first"])).await;

    let handlers = projecting(COPY);
    messaging.register(&handlers).await.unwrap();
    run(&messaging.worker_of(handlers)).await;

    assert_eq!(rows(&messaging).await, ["alice ponged v2", "bob pinged v1"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn an_event_older_than_the_copy_changes_nothing(pool: PgPool) {
    let messaging = with_copy_table(pool).await;
    let handlers = projecting(COPY);

    deliver(&messaging, &handlers, pong("alice", 2)).await;
    deliver(&messaging, &handlers, ping("alice", 1)).await;
    deliver(&messaging, &handlers, pong("alice", 2)).await;

    assert_eq!(rows(&messaging).await, ["alice ponged v2"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_removed_key_stays_removed_when_an_older_event_arrives(pool: PgPool) {
    let messaging = with_copy_table(pool).await;
    let handlers = projecting(COPY);
    deliver(&messaging, &handlers, ping("alice", 1)).await;
    deliver(&messaging, &handlers, ping("bob", 1)).await;

    deliver(&messaging, &handlers, drop("alice", 3)).await;
    deliver(&messaging, &handlers, pong("alice", 2)).await;

    assert_eq!(rows(&messaging).await, ["bob pinged v1"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_projection_has_caught_up_once_its_backfill_is_copied_and_handled(pool: PgPool) {
    let messaging = with_copy_table(pool).await;
    messaging.migrate(&["tests.first"]).await;
    messaging.publish(vec![ping("alice", 1)]).await.unwrap();
    run(&messaging.worker(&["tests.first"])).await;
    let ready = || async { caught_up(&messaging.pool, &[COPY]).await.unwrap() };
    assert!(!ready().await, "not registered yet");

    let handlers = projecting(COPY);
    messaging.register(&handlers).await.unwrap();
    assert!(!ready().await, "backfill not copied yet");

    let worker = messaging.worker_of(handlers);
    worker.backfill().await.unwrap();
    worker.backfill().await.unwrap();
    assert!(!ready().await, "backfill copied but not handled");

    run(&worker).await;
    assert!(ready().await);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_latest_subscription_has_nothing_to_catch_up_with(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(&["tests.first"]).await;

    assert!(caught_up(&messaging.pool, &["tests.first"]).await.unwrap());
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn nothing_is_served_until_a_generation_has_caught_up(pool: PgPool) {
    let messaging = with_copy_table(pool).await;
    let handlers = projecting(COPY);
    messaging.register(&handlers).await.unwrap();
    let generations = Generations::new(messaging.pool.clone()).then(1, &[COPY]);

    assert!(matches!(
        generations.serving().await,
        Err(AppError::Unavailable(_))
    ));
    run(&messaging.worker_of(handlers)).await;
    assert_eq!(generations.serving().await.unwrap(), 1);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_rebuilt_generation_is_served_once_it_catches_up_and_the_old_one_until_then(
    pool: PgPool,
) {
    let messaging = with_copy_table(pool).await;
    let old = projecting(COPY);
    messaging.register(&old).await.unwrap();
    run(&messaging.worker_of(old)).await;
    messaging.publish(vec![ping("alice", 1)]).await.unwrap();
    let generations = Generations::new(messaging.pool.clone())
        .then(2, &[COPY_V2])
        .then(1, &[COPY]);

    let both = || {
        projecting(COPY).merge(
            Handlers::new().on_from_earliest::<PingTopic>(COPY_V2, messaging.recorder(COPY_V2)),
        )
    };
    messaging.register(&both()).await.unwrap();
    messaging.fails("alice", 1, 1);
    run(&messaging.worker_of(both())).await;
    assert_eq!(generations.serving().await.unwrap(), 1);

    messaging.later(TimeDelta::minutes(1));
    run(&messaging.worker_of(both())).await;
    assert_eq!(generations.serving().await.unwrap(), 2);
}
