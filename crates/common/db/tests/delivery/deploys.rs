use crate::support::{Job, Messaging, Poked, run};
use app_db::SubscriptionError;
use sqlx::PgPool;
use std::future::ready;
use std::time::Duration;

const OLD: &[&str] = &["tests.first"];
const NEW: &[&str] = &["tests.first", "tests.second"];

#[sqlx::test(migrations = false)]
async fn an_old_worker_leaves_a_subscription_it_doesnt_serve_to_one_that_does(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(NEW).await;
    let (old, new) = (messaging.worker(OLD), messaging.worker(NEW));
    messaging.ping("alice").await;
    new.relay().await.expect("relay");

    run(&old).await;

    assert_eq!(messaging.log.handled(), ["tests.first: alice"]);
    assert_eq!(messaging.jobs().await, [Job::untouched("tests.second")]);

    run(&new).await;

    assert_eq!(
        messaging.log.handled(),
        ["tests.first: alice", "tests.second: alice"]
    );
    assert!(messaging.jobs().await.is_empty());
}

#[sqlx::test(migrations = false)]
async fn an_old_workers_relay_fans_out_to_a_subscription_registered_since(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(OLD).await;
    let (old, new) = (messaging.worker(OLD), messaging.worker(NEW));
    messaging.migrate(NEW).await;
    messaging.ping("alice").await;

    old.relay().await.expect("relay");
    run(&new).await;

    assert_eq!(
        messaging.log.handled(),
        ["tests.first: alice", "tests.second: alice"]
    );
}

#[sqlx::test(migrations = false)]
async fn a_subscription_gets_no_event_relayed_before_it_was_registered(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(OLD).await;
    messaging.ping("alice").await;
    messaging.worker(OLD).relay().await.expect("relay");

    messaging.migrate(NEW).await;
    messaging.ping("bob").await;
    run(&messaging.worker(NEW)).await;

    assert_eq!(
        messaging.log.handled(),
        [
            "tests.first: alice",
            "tests.first: bob",
            "tests.second: bob"
        ]
    );
}

#[sqlx::test(migrations = false)]
async fn registering_again_changes_nothing(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(NEW).await;
    messaging.migrate(NEW).await;
    messaging.ping("alice").await;

    run(&messaging.worker(NEW)).await;

    assert_eq!(
        messaging.log.handled(),
        ["tests.first: alice", "tests.second: alice"]
    );
}

#[sqlx::test(migrations = false)]
async fn a_queue_cannot_be_registered_under_a_second_topic(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(&["tests.q"]).await;
    let other_topic =
        app_persistence::Handlers::new().on::<Poked>("tests.q", messaging.recorder("tests.q"));

    let refused = messaging.register(&other_topic).await;

    assert!(
        matches!(refused, Err(SubscriptionError::TopicChanged { .. })),
        "{refused:?}"
    );
}

#[sqlx::test(migrations = false)]
async fn retiring_a_subscription_dead_letters_what_it_hadnt_run_and_relays_it_nothing_more(
    pool: PgPool,
) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(NEW).await;
    let worker = messaging.worker(NEW);
    messaging.ping("alice").await;
    worker.relay().await.expect("relay");

    messaging.retire("tests.second").await;
    messaging.ping("bob").await;
    run(&worker).await;

    assert_eq!(
        messaging.log.handled(),
        ["tests.first: alice", "tests.first: bob"]
    );
    assert_eq!(messaging.jobs().await, [Job::retired("tests.second")]);
}

#[sqlx::test(migrations = false)]
async fn a_retired_queue_cannot_be_registered_again(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(NEW).await;
    messaging.retire("tests.second").await;

    let refused = messaging.register(&messaging.handlers(NEW)).await;

    assert!(
        matches!(refused, Err(SubscriptionError::Retired(_))),
        "{refused:?}"
    );
}

#[sqlx::test(migrations = false)]
async fn a_worker_whose_subscriptions_arent_registered_refuses_to_start(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(OLD).await;

    let started = messaging.worker(NEW).run(Duration::ZERO, ready(())).await;

    assert!(started.is_err());
    assert!(
        messaging
            .worker(OLD)
            .run(Duration::ZERO, ready(()))
            .await
            .is_ok()
    );
}
