use crate::support::{Job, Messaging};
use app_persistence::MAX_ATTEMPTS;
use chrono::TimeDelta;
use sqlx::PgPool;
use std::time::Duration;

const QUEUE: &[&str] = &["tests.first"];
const SOON: Duration = Duration::from_secs(10);

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_drain_returns_once_nothing_is_due_leaving_what_is_due_later(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    let worker = messaging.worker(QUEUE);
    messaging.fails("alice", 1, 1);
    messaging.ping("alice").await;
    messaging.ping("bob").await;

    tokio::time::timeout(SOON, worker.drain(None))
        .await
        .expect("returns once idle")
        .expect("drains");

    assert_eq!(messaging.log.handled(), ["tests.first: pinged bob v1"]);
    assert_eq!(
        messaging.jobs().await,
        [Job {
            attempts: 1,
            last_error: Some("conflict: not now".into()),
            ..Job::untouched("tests.first")
        }]
    );

    messaging.later(TimeDelta::hours(1));
    worker.drain(None).await.expect("drains");

    assert_eq!(
        messaging.log.handled(),
        ["tests.first: pinged bob v1", "tests.first: pinged alice v1"]
    );
    assert!(messaging.jobs().await.is_empty());
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_drain_stops_once_its_budget_is_spent(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    let worker = messaging.worker(QUEUE);
    messaging.hangs("alice", 1);
    messaging.ping("bob").await;
    messaging.ping("alice").await;
    messaging.ping("carol").await;

    tokio::time::timeout(SOON, worker.drain(Some(Duration::from_millis(500))))
        .await
        .expect("returns once its budget is spent")
        .expect("drains");

    assert_eq!(messaging.log.handled(), ["tests.first: pinged bob v1"]);
    assert_eq!(
        messaging.jobs().await,
        [
            Job {
                attempts: 1,
                ..Job::untouched("tests.first")
            },
            Job::untouched("tests.first"),
        ]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_failing_job_is_retried_and_dead_lettered_by_drains_that_still_succeed(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    let worker = messaging.worker(QUEUE);
    messaging.fails("alice", 1, u32::MAX);
    messaging.ping("alice").await;

    for _ in 0..MAX_ATTEMPTS {
        worker.drain(None).await.expect("drains");
        messaging.later(TimeDelta::hours(1));
    }

    let dead = worker.dead_letters().await.unwrap();
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].attempts, MAX_ATTEMPTS);
    assert_eq!(dead[0].last_error.as_deref(), Some("conflict: not now"));
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_drain_that_cannot_reach_the_database_fails(pool: PgPool) {
    let messaging = Messaging::new(pool.clone()).await;
    messaging.migrate(QUEUE).await;
    let worker = messaging.worker(QUEUE);
    pool.close().await;

    assert!(worker.drain(None).await.is_err());
}
