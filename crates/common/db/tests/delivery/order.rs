use crate::support::{Messaging, PingTopic, Pinged, ping, pong, run};
use app_persistence::Handlers;
use chrono::TimeDelta;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::collections::BTreeMap;

const QUEUE: &[&str] = &["tests.first"];

#[sqlx::test(migrations = false)]
async fn a_keys_next_version_waits_until_a_failing_one_succeeds(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    let worker = messaging.worker(QUEUE);
    messaging.fails("alice", 2, 1);
    let versions = (1..=3).map(|v| ping("alice", v)).collect();
    messaging.publish(versions).await.unwrap();

    run(&worker).await;
    assert_eq!(messaging.log.handled(), ["tests.first: pinged alice v1"]);

    messaging.later(TimeDelta::minutes(1));
    run(&worker).await;
    assert_eq!(
        messaging.log.handled(),
        [
            "tests.first: pinged alice v1",
            "tests.first: pinged alice v2",
            "tests.first: pinged alice v3",
        ]
    );
}

#[sqlx::test(migrations = false)]
async fn a_dead_lettered_version_no_longer_holds_up_its_key(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    let worker = messaging.worker(QUEUE);
    messaging.fails("alice", 2, u32::MAX);
    let versions = (1..=3).map(|v| ping("alice", v)).collect();
    messaging.publish(versions).await.unwrap();

    for _ in 0..4 {
        run(&worker).await;
        messaging.later(TimeDelta::hours(1));
    }
    assert_eq!(messaging.log.handled(), ["tests.first: pinged alice v1"]);

    run(&worker).await;

    assert_eq!(
        messaging.log.handled(),
        [
            "tests.first: pinged alice v1",
            "tests.first: pinged alice v3"
        ]
    );
    assert_eq!(worker.dead_letters().await.unwrap().len(), 1);
}

#[sqlx::test(migrations = false)]
async fn another_key_proceeds_while_one_is_held_up(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    messaging.fails("alice", 1, 1);
    messaging
        .publish(vec![ping("alice", 1), ping("alice", 2), ping("bob", 1)])
        .await
        .unwrap();

    run(&messaging.worker(QUEUE)).await;

    assert_eq!(messaging.log.handled(), ["tests.first: pinged bob v1"]);
}

#[sqlx::test(migrations = false)]
async fn a_whole_topic_queue_gets_a_keys_events_of_every_type_in_version_order(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let handlers = Handlers::new().on::<PingTopic>("tests.all", messaging.recorder("tests.all"));
    messaging.register(&handlers).await.unwrap();
    let worker = messaging.worker_of(handlers);
    messaging.fails("alice", 1, 1);
    messaging
        .publish(vec![ping("alice", 1), pong("alice", 2)])
        .await
        .unwrap();

    run(&worker).await;
    assert!(messaging.log.handled().is_empty());

    messaging.later(TimeDelta::minutes(1));
    run(&worker).await;
    assert_eq!(
        messaging.log.handled(),
        ["tests.all: pinged alice v1", "tests.all: ponged alice v2"]
    );
}

/// The old build filtered `tests.all` to pings; the new one takes its whole topic.
#[sqlx::test(migrations = false)]
async fn a_job_no_running_worker_can_decode_holds_up_only_its_key_in_its_queue(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let new = Handlers::new()
        .on::<PingTopic>("tests.all", messaging.recorder("tests.all"))
        .on::<Pinged>("tests.first", messaging.recorder("tests.first"));
    messaging.register(&new).await.unwrap();
    let old = messaging.worker_of(
        Handlers::new()
            .on::<Pinged>("tests.all", messaging.recorder("tests.all"))
            .on::<Pinged>("tests.first", messaging.recorder("tests.first")),
    );
    messaging
        .publish(vec![ping("alice", 1), pong("alice", 2), ping("bob", 1)])
        .await
        .unwrap();
    messaging.publish(vec![ping("alice", 3)]).await.unwrap();

    run(&old).await;
    assert_eq!(
        messaging.log.handled(),
        [
            "tests.all: pinged alice v1",
            "tests.first: pinged alice v1",
            "tests.all: pinged bob v1",
            "tests.first: pinged bob v1",
            "tests.first: pinged alice v3",
        ]
    );

    run(&messaging.worker_of(new)).await;
    assert_eq!(
        messaging.log.handled()[5..],
        ["tests.all: ponged alice v2", "tests.all: pinged alice v3"]
    );
}

#[sqlx::test(migrations = false)]
async fn relays_running_at_once_keep_each_keys_versions_in_order_and_fan_out_once(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(&["tests.first", "tests.second"]).await;
    let keys = ["alice", "bob", "carol"];
    for version in 1..=150 {
        let events = keys.iter().map(|key| ping(key, version)).collect();
        messaging.publish(events).await.unwrap();
    }
    let relayed = relay_in_parallel(&messaging, 4);

    assert_eq!(relayed, 450);
    let jobs: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT queue, key, version FROM messaging.job ORDER BY id")
            .fetch_all(&messaging.pool)
            .await
            .unwrap();
    let mut by_key: BTreeMap<(String, String), Vec<i64>> = BTreeMap::new();
    for (queue, key, version) in jobs {
        by_key.entry((queue, key)).or_default().push(version);
    }
    assert_eq!(by_key.len(), 6);
    for versions in by_key.values() {
        assert_eq!(versions, &(1..=150).collect::<Vec<_>>());
    }
}

/// `relays` workers, each on its own thread, runtime and pool, relaying until
/// they keep finding nothing to do; how many events they relayed in all.
fn relay_in_parallel(messaging: &Messaging, relays: usize) -> usize {
    let options = messaging.pool.connect_options();
    std::thread::scope(|threads| {
        let relays: Vec<_> = (0..relays)
            .map(|_| {
                let options = (*options).clone();
                threads.spawn(|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    runtime.block_on(async {
                        let pool = PgPoolOptions::new()
                            .max_connections(1)
                            .connect_with(options)
                            .await
                            .unwrap();
                        let worker = messaging.worker_on(pool);
                        let (mut relayed, mut idle) = (0, 0);
                        while idle < 20 {
                            match worker.relay().await.expect("relay") {
                                0 => idle += 1,
                                n => relayed += n,
                            }
                        }
                        relayed
                    })
                })
            })
            .collect();
        relays.into_iter().map(|relay| relay.join().unwrap()).sum()
    })
}
