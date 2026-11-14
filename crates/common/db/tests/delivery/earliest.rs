use crate::support::{Messaging, PingTopic, Pinged, ping, pong, run};
use app_events::PublishedTopic;
use app_persistence::Handlers;
use chrono::TimeDelta;
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, Ordering};

const OLD: &[&str] = &["tests.first"];
const COPY: &str = "tests.copy";
/// Of each key, published while `tests.copy` is registered.
const VERSIONS: i64 = 60;

/// A build whose `tests.copy` keeps a copy of every Ping, from the earliest.
fn copying(messaging: &Messaging) -> Handlers {
    Handlers::new().on_from_earliest::<PingTopic>(COPY, messaging.recorder(COPY))
}

/// The old build has run: its subscription has handled `events`.
async fn history(messaging: &Messaging, events: Vec<app_persistence::PendingEvent>) {
    messaging.migrate(OLD).await;
    messaging.publish(events).await.unwrap();
    run(&messaging.worker(OLD)).await;
}

fn copied(messaging: &Messaging) -> Vec<String> {
    messaging
        .log
        .handled()
        .into_iter()
        .filter(|line| line.starts_with(COPY))
        .collect()
}

fn copied_of(messaging: &Messaging, key: &str) -> Vec<String> {
    let key = format!(" {key} v");
    copied(messaging)
        .into_iter()
        .filter(|line| line.contains(&key))
        .collect()
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_new_earliest_subscription_receives_every_event_its_topic_holds_in_version_order_per_key(
    pool: PgPool,
) {
    let messaging = Messaging::new(pool).await;
    history(
        &messaging,
        vec![pong("alice", 1), ping("bob", 1), ping("alice", 2)],
    )
    .await;

    let handlers = copying(&messaging);
    messaging.register(&handlers).await.unwrap();
    run(&messaging.worker_of(handlers)).await;

    assert_eq!(
        copied_of(&messaging, "alice"),
        ["tests.copy: ponged alice v1", "tests.copy: pinged alice v2"]
    );
    assert_eq!(copied_of(&messaging, "bob"), ["tests.copy: pinged bob v1"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_live_event_waits_for_its_keys_backfilled_events_and_runs_after_them(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    history(&messaging, vec![ping("alice", 1), ping("alice", 2)]).await;
    let handlers = copying(&messaging);
    messaging.register(&handlers).await.unwrap();
    let worker = messaging.worker_of(handlers);
    messaging.fails("alice", 1, 1);
    messaging.publish(vec![ping("alice", 3)]).await.unwrap();

    run(&worker).await;
    assert!(copied(&messaging).is_empty());

    messaging.later(TimeDelta::minutes(1));
    run(&worker).await;
    assert_eq!(
        copied(&messaging),
        [
            "tests.copy: pinged alice v1",
            "tests.copy: pinged alice v2",
            "tests.copy: pinged alice v3",
        ]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_live_event_of_another_key_doesnt_wait_for_the_backfill(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    history(&messaging, vec![ping("alice", 1)]).await;
    let handlers = copying(&messaging);
    messaging.register(&handlers).await.unwrap();
    messaging.fails("alice", 1, 1);
    messaging.publish(vec![ping("bob", 1)]).await.unwrap();

    run(&messaging.worker_of(handlers)).await;

    assert_eq!(copied(&messaging), ["tests.copy: pinged bob v1"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn after_compaction_it_receives_each_keys_latest_event_and_the_history_within_the_retention(
    pool: PgPool,
) {
    let messaging = Messaging::new(pool).await;
    messaging
        .register_topics(&[PublishedTopic::of::<PingTopic>()])
        .await;
    history(&messaging, vec![ping("alice", 1), ping("bob", 1)]).await;
    messaging.later(TimeDelta::days(8));
    history(&messaging, vec![pong("alice", 2)]).await;
    messaging.later(TimeDelta::days(1));
    history(&messaging, vec![ping("alice", 3)]).await;
    messaging.worker(OLD).compact().await.unwrap();

    let handlers = copying(&messaging);
    messaging.register(&handlers).await.unwrap();
    run(&messaging.worker_of(handlers)).await;

    assert_eq!(
        copied(&messaging),
        [
            "tests.copy: pinged bob v1",
            "tests.copy: ponged alice v2",
            "tests.copy: pinged alice v3",
        ]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_latest_subscription_receives_only_later_events(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    history(&messaging, vec![ping("alice", 1)]).await;
    let handlers = Handlers::new().on::<PingTopic>(COPY, messaging.recorder(COPY));
    messaging.register(&handlers).await.unwrap();
    messaging.publish(vec![ping("bob", 1)]).await.unwrap();

    run(&messaging.worker_of(handlers)).await;

    assert_eq!(copied(&messaging), ["tests.copy: pinged bob v1"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn registering_again_doesnt_backfill_again(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    history(&messaging, vec![ping("alice", 1)]).await;
    let handlers = copying(&messaging);
    messaging.register(&handlers).await.unwrap();
    let worker = messaging.worker_of(copying(&messaging));
    run(&worker).await;

    messaging.register(&handlers).await.unwrap();
    run(&worker).await;

    assert_eq!(copied(&messaging), ["tests.copy: pinged alice v1"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn declaring_a_registered_latest_subscription_earliest_doesnt_backfill_it(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let latest = Handlers::new().on::<PingTopic>(COPY, messaging.recorder(COPY));
    messaging.register(&latest).await.unwrap();
    history(&messaging, vec![ping("alice", 1)]).await;
    let worker = messaging.worker_of(latest);
    run(&worker).await;

    messaging.register(&copying(&messaging)).await.unwrap();
    run(&worker).await;

    assert_eq!(copied(&messaging), ["tests.copy: pinged alice v1"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn an_earliest_subscription_to_one_event_type_receives_only_that_type(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    history(
        &messaging,
        vec![ping("alice", 1), pong("bob", 1), pong("alice", 2)],
    )
    .await;
    let handlers = Handlers::new().on_from_earliest::<Pinged>(COPY, messaging.recorder(COPY));
    messaging.register(&handlers).await.unwrap();

    run(&messaging.worker_of(handlers)).await;

    assert_eq!(copied(&messaging), ["tests.copy: pinged alice v1"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn an_event_published_while_it_is_registered_is_delivered_once(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(OLD).await;
    let relay = messaging.worker_on(messaging.pool.clone());
    let relayed = AtomicI64::new(0);
    let keys = ["alice", "bob", "carol"];
    let publishing = async {
        for version in 1..=VERSIONS {
            let events = keys.iter().map(|key| ping(key, version)).collect();
            messaging.publish(events).await.unwrap();
            relay.relay().await.unwrap();
            relayed.store(version, Ordering::SeqCst);
        }
    };
    let handlers = copying(&messaging);
    let registering = async {
        while relayed.load(Ordering::SeqCst) < VERSIONS / 3 {
            tokio::task::yield_now().await;
        }
        messaging.register(&handlers).await.unwrap();
    };
    tokio::join!(publishing, registering);

    run(&messaging.worker_of(copying(&messaging))).await;

    let mut by_key: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for line in copied(&messaging) {
        let (key, version) = line
            .strip_prefix("tests.copy: pinged ")
            .and_then(|rest| rest.split_once(" v"))
            .unwrap();
        by_key
            .entry(key.into())
            .or_default()
            .push(version.parse().unwrap());
    }
    assert_eq!(by_key.len(), keys.len());
    for versions in by_key.values() {
        assert_eq!(versions, &(1..=VERSIONS).collect::<Vec<_>>());
    }
}
