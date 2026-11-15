use crate::support::{Job, Messaging, PingTopic, Pinged, ping, pong, run};
use app_db::caught_up;
use app_events::PublishedTopic;
use app_persistence::Handlers;
use chrono::TimeDelta;
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

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

#[app_test_db::test(crate::support::TEMPLATE)]
async fn registering_copies_nothing_and_a_worker_copies_the_backfill(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    history(&messaging, vec![ping("alice", 1), ping("bob", 1)]).await;
    let handlers = copying(&messaging);
    messaging.register(&handlers).await.unwrap();
    let queued = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM messaging.job WHERE queue = $1")
            .bind(COPY)
            .fetch_one(&messaging.pool)
            .await
            .unwrap()
    };
    assert_eq!(queued().await, 0);

    let copied = messaging.worker_of(handlers).backfill().await.unwrap();

    assert_eq!(copied, 2);
    assert_eq!(queued().await, 2);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_backfill_with_no_relayed_events_of_its_type_is_caught_up_once_registered(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    history(&messaging, vec![pong("alice", 1)]).await;
    let handlers = Handlers::new().on_from_earliest::<Pinged>(COPY, messaging.recorder(COPY));

    messaging.register(&handlers).await.unwrap();
    let ready = caught_up(&messaging.pool, &[COPY]).await.unwrap();
    let copied = messaging.worker_of(handlers).backfill().await.unwrap();

    assert!(ready, "not caught up straight after registration");
    assert_eq!(copied, 0);
}

/// Every job of `queue` still to run, `<key> v<version>`, in the order copied.
async fn queued(messaging: &Messaging, queue: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT key || ' v' || version FROM messaging.job WHERE queue = $1 ORDER BY id",
    )
    .bind(queue)
    .fetch_all(&messaging.pool)
    .await
    .unwrap()
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_backfill_spanning_several_chunks_is_copied_a_chunk_at_a_time_in_version_order(
    pool: PgPool,
) {
    let messaging = Messaging::new(pool).await;
    history(
        &messaging,
        vec![
            ping("alice", 1),
            ping("bob", 1),
            pong("alice", 2),
            ping("bob", 2),
            ping("alice", 3),
        ],
    )
    .await;
    let handlers = copying(&messaging);
    messaging.register(&handlers).await.unwrap();
    let worker = messaging
        .worker_of(copying(&messaging))
        .backfilling_in_chunks_of(2);

    let mut chunks = Vec::new();
    loop {
        match worker.backfill().await.unwrap() {
            0 => break,
            copied => chunks.push(copied),
        }
    }
    run(&worker).await;

    assert_eq!(chunks, [2, 2, 1]);
    assert_eq!(
        copied_of(&messaging, "alice"),
        [
            "tests.copy: pinged alice v1",
            "tests.copy: ponged alice v2",
            "tests.copy: pinged alice v3",
        ]
    );
    assert_eq!(
        copied_of(&messaging, "bob"),
        ["tests.copy: pinged bob v1", "tests.copy: pinged bob v2"]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_worker_stopping_mid_backfill_leaves_the_rest_to_another_and_nothing_is_copied_twice(
    pool: PgPool,
) {
    let messaging = Messaging::new(pool).await;
    history(
        &messaging,
        vec![ping("alice", 1), ping("alice", 2), ping("bob", 1)],
    )
    .await;
    let handlers = copying(&messaging);
    messaging.register(&handlers).await.unwrap();
    let stopping = messaging.worker_of(handlers).backfilling_in_chunks_of(2);
    assert_eq!(stopping.backfill().await.unwrap(), 2);
    drop(stopping);
    messaging.publish(vec![ping("alice", 3)]).await.unwrap();

    run(&messaging
        .worker_of(copying(&messaging))
        .backfilling_in_chunks_of(2))
    .await;

    assert_eq!(
        copied_of(&messaging, "alice"),
        [
            "tests.copy: pinged alice v1",
            "tests.copy: pinged alice v2",
            "tests.copy: pinged alice v3",
        ]
    );
    assert_eq!(copied_of(&messaging, "bob"), ["tests.copy: pinged bob v1"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn workers_copying_backfills_at_once_copy_each_event_once(pool: PgPool) {
    const COPY_TOO: &str = "tests.copy_too";
    let messaging = Messaging::new(pool).await;
    let events: Vec<_> = ["alice", "bob", "carol"]
        .iter()
        .flat_map(|key| (1..=10).map(move |version| ping(key, version)))
        .collect();
    history(&messaging, events).await;
    let both = || {
        copying(&messaging).on_from_earliest::<PingTopic>(COPY_TOO, messaging.recorder(COPY_TOO))
    };
    messaging.register(&both()).await.unwrap();
    let first = messaging.worker_of(both()).backfilling_in_chunks_of(1);
    let second = messaging.worker_of(both()).backfilling_in_chunks_of(1);
    let (by_first, by_second) = tokio::join!(copy_all(&first), copy_all(&second));

    assert_eq!(by_first + by_second, 60);
    for queue in [COPY, COPY_TOO] {
        let mut jobs = queued(&messaging, queue).await;
        assert_eq!(jobs.len(), 30, "{queue}");
        jobs.sort();
        jobs.dedup();
        assert_eq!(jobs.len(), 30, "{queue} copied an event twice");
    }
}

/// Copies backfill chunks until none is left; how many events it copied.
async fn copy_all(worker: &app_db::Worker) -> u64 {
    let mut copied = 0;
    loop {
        match worker.backfill().await.unwrap() {
            0 => return copied,
            n => copied += n,
        }
    }
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn retiring_waits_for_a_chunk_being_copied_and_dead_letters_it(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    history(&messaging, vec![ping("alice", 1)]).await;
    messaging.register(&copying(&messaging)).await.unwrap();
    let mut copying_a_chunk = messaging.pool.begin().await.unwrap();
    sqlx::query(
        "SELECT 1 FROM messaging.backfill WHERE queue = $1 AND event_type = 'tests.pinged' \
         FOR UPDATE",
    )
    .bind(COPY)
    .execute(&mut *copying_a_chunk)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO messaging.job \
             (queue, event_type, key, version, payload, outbox_seq, run_at, max_attempts, \
              created_at) \
         SELECT $1, event_type, key, version, payload, seq, now(), 5, now() \
         FROM messaging.outbox",
    )
    .bind(COPY)
    .execute(&mut *copying_a_chunk)
    .await
    .unwrap();

    let retire = messaging.retire(COPY);
    tokio::pin!(retire);
    let waited = tokio::time::timeout(Duration::from_millis(200), &mut retire).await;
    assert!(waited.is_err(), "retiring didn't wait for the chunk");
    copying_a_chunk.commit().await.unwrap();
    retire.await;

    assert_eq!(messaging.jobs().await, [Job::retired(COPY)]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn retiring_completes_the_backfill_and_copies_nothing_more(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    history(&messaging, vec![ping("alice", 1), ping("bob", 1)]).await;
    messaging.register(&copying(&messaging)).await.unwrap();
    messaging.retire(COPY).await;
    let completed: bool = sqlx::query_scalar(
        "SELECT bool_and(completed_at IS NOT NULL) FROM messaging.backfill WHERE queue = $1",
    )
    .bind(COPY)
    .fetch_one(&messaging.pool)
    .await
    .unwrap();

    let copied = messaging
        .worker_of(copying(&messaging))
        .backfill()
        .await
        .unwrap();

    assert!(completed, "retiring left its backfill unfinished");
    assert_eq!(copied, 0);
    assert_eq!(messaging.jobs().await, []);
}
