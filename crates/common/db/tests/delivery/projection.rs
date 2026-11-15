use crate::support::{LoggedErrors, Messaging, PingEvent, PingTopic, drop, ping, pong, run};
use app_db::{Generations, Project, Projecting, caught_up, conn, dead_letters_in};
use app_domain::seed_time;
use app_error::{AppError, AppResult};
use app_events::TopicEnvelope;
use app_persistence::{Handlers, MAX_ATTEMPTS, PendingEvent, Transactions, Tx};
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

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_projection_waits_for_the_events_still_unrelayed_when_it_was_registered(pool: PgPool) {
    let messaging = with_copy_table(pool).await;
    let events: Vec<_> = (0..250).map(|i| ping(&format!("key{i}"), 1)).collect();
    messaging.publish(events).await.unwrap();
    let handlers = projecting(COPY);
    messaging.register(&handlers).await.unwrap();
    let worker = messaging.worker_of(handlers);

    // One pass of `run_until_idle`'s loop.
    assert_eq!(worker.relay().await.unwrap(), 100);
    worker.backfill().await.unwrap();
    while worker.run_next().await.unwrap() {}

    let held = rows(&messaging).await.len();
    let ready = caught_up(&messaging.pool, &[COPY]).await.unwrap();
    assert!(!ready, "caught up with {held} of 250 keys");
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_projection_waits_for_its_last_copied_chunk_whatever_the_workers_clocks(pool: PgPool) {
    let messaging = with_copy_table(pool).await;
    messaging.migrate(&["tests.first"]).await;
    messaging
        .publish(vec![ping("alice", 1), ping("bob", 1)])
        .await
        .unwrap();
    run(&messaging.worker(&["tests.first"])).await;
    messaging.register(&projecting(COPY)).await.unwrap();
    let worker = messaging
        .worker_of(projecting(COPY))
        .backfilling_in_chunks_of(1);

    assert_eq!(worker.backfill().await.unwrap(), 1);
    assert!(worker.run_next().await.unwrap());
    // Worker A copies the last chunk; worker B, 5ms behind, finds none left.
    messaging.later(TimeDelta::milliseconds(10));
    assert_eq!(worker.backfill().await.unwrap(), 1);
    messaging.later(TimeDelta::milliseconds(-5));
    assert_eq!(worker.backfill().await.unwrap(), 0);

    let held = rows(&messaging).await.len();
    let ready = caught_up(&messaging.pool, &[COPY]).await.unwrap();
    assert!(
        !ready,
        "caught up holding {held} of 2 keys, a job unhandled"
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn an_event_published_after_registration_does_not_hold_a_projection_back(pool: PgPool) {
    let messaging = with_copy_table(pool).await;
    messaging.ping("alice").await;
    let handlers = projecting(COPY);
    messaging.register(&handlers).await.unwrap();
    run(&messaging.worker_of(handlers)).await;

    messaging.ping("bob").await;

    assert!(caught_up(&messaging.pool, &[COPY]).await.unwrap());
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn an_event_relayed_after_registration_does_not_hold_a_projection_back(pool: PgPool) {
    let messaging = with_copy_table(pool).await;
    messaging.ping("alice").await;
    let handlers = projecting(COPY);
    messaging.register(&handlers).await.unwrap();
    let worker = messaging.worker_of(handlers);
    run(&worker).await;

    messaging.ping("bob").await;
    assert_eq!(worker.relay().await.unwrap(), 1);

    assert!(caught_up(&messaging.pool, &[COPY]).await.unwrap());
}

fn recording(messaging: &Messaging, queue: &'static str) -> Handlers {
    Handlers::new().on_from_earliest::<PingTopic>(queue, messaging.recorder(queue))
}

/// `COPY` backfilled with alice's and bob's pings, alice's dead-lettered.
async fn a_copy_with_a_dead_letter(messaging: &Messaging) {
    messaging
        .publish(vec![ping("alice", 1), ping("bob", 1)])
        .await
        .unwrap();
    messaging
        .register(&recording(messaging, COPY))
        .await
        .unwrap();
    messaging.fails("alice", 1, MAX_ATTEMPTS.unsigned_abs());
    let worker = messaging.worker_of(recording(messaging, COPY));
    for _ in 0..MAX_ATTEMPTS {
        run(&worker).await;
        messaging.later(TimeDelta::hours(1));
    }
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_projection_with_a_dead_lettered_backfill_delivery_is_served_and_reports_it(
    pool: PgPool,
) {
    let messaging = Messaging::new(pool).await;
    a_copy_with_a_dead_letter(&messaging).await;
    let generations = Generations::new(messaging.pool.clone()).then(1, &[COPY]);

    assert_eq!(generations.serving().await.unwrap(), 1);
    assert_eq!(
        dead_letters_in(&messaging.pool, &[COPY]).await.unwrap(),
        [(COPY.to_owned(), 1)]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn another_queues_dead_letters_are_not_reported_for_a_projection(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    a_copy_with_a_dead_letter(&messaging).await;

    messaging
        .register(&recording(&messaging, COPY_V2))
        .await
        .unwrap();
    run(&messaging.worker_of(recording(&messaging, COPY_V2))).await;

    assert_eq!(
        dead_letters_in(&messaging.pool, &[COPY_V2]).await.unwrap(),
        []
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn serving_a_projection_with_dead_letters_is_logged_as_an_error_once(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    a_copy_with_a_dead_letter(&messaging).await;
    let generations = Generations::new(messaging.pool.clone()).then(1, &[COPY]);
    let (errors, _capturing) = LoggedErrors::capture();

    generations.serving().await.unwrap();
    generations.serving().await.unwrap();

    assert_eq!(
        errors.lines(),
        [
            "serving a projection with dead-lettered deliveries: its copy is missing or stale \
             for their keys until they are rerun queue=\"tests.copy\" dead_letters=1"
        ]
    );
}
