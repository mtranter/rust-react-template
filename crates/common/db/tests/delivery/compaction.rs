use crate::support::{Job, Messaging, PingTopic, PokeTopic, drop, ping, poke, run};
use app_events::{PublishedTopic, Topic};
use chrono::TimeDelta;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

const QUEUE: &[&str] = &["tests.first"];

/// A Ping topic keeps its history for 7 days.
async fn messaging_keeping_pings(pool: PgPool) -> Messaging {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    messaging
        .register_topics(&[PublishedTopic::of::<PingTopic>()])
        .await;
    messaging
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_keys_versions_older_than_the_retention_are_deleted_but_its_latest_stays(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    let worker = messaging.worker(QUEUE);
    let versions = (1..=3).map(|v| ping("alice", v)).collect();
    messaging.publish(versions).await.unwrap();
    run(&worker).await;

    messaging.later(TimeDelta::days(8));
    let deleted = worker.compact().await.unwrap();

    assert_eq!(deleted, 2);
    assert_eq!(messaging.outbox().await, ["tests.pinged alice v3"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_keys_only_version_stays_however_old(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    let worker = messaging.worker(QUEUE);
    messaging.ping("alice").await;
    run(&worker).await;

    messaging.later(TimeDelta::days(30));
    worker.compact().await.unwrap();

    assert_eq!(messaging.outbox().await, ["tests.pinged alice v1"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_deleted_roots_last_event_stays_however_old(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    let worker = messaging.worker(QUEUE);
    messaging
        .publish(vec![ping("alice", 1), drop("alice", 2)])
        .await
        .unwrap();
    run(&worker).await;

    messaging.later(TimeDelta::days(30));
    worker.compact().await.unwrap();

    assert_eq!(messaging.outbox().await, ["tests.dropped alice v2"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn versions_within_the_retention_stay(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    let worker = messaging.worker(QUEUE);
    messaging.ping("alice").await;
    messaging.later(TimeDelta::days(3));
    messaging.publish(vec![ping("alice", 2)]).await.unwrap();
    messaging.later(TimeDelta::days(3));
    messaging.publish(vec![ping("alice", 3)]).await.unwrap();
    run(&worker).await;

    messaging.later(TimeDelta::days(3));
    worker.compact().await.unwrap();

    assert_eq!(
        messaging.outbox().await,
        ["tests.pinged alice v2", "tests.pinged alice v3"]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn an_event_not_yet_relayed_stays_however_old(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    messaging
        .publish(vec![ping("alice", 1), ping("alice", 2)])
        .await
        .unwrap();

    messaging.later(TimeDelta::days(30));
    messaging.worker(QUEUE).compact().await.unwrap();

    assert_eq!(
        messaging.outbox().await,
        ["tests.pinged alice v1", "tests.pinged alice v2"]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn each_topic_is_compacted_by_its_own_retention(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    messaging
        .register_topics(&[PublishedTopic::of::<PokeTopic>()])
        .await;
    let worker = messaging.worker(QUEUE);
    messaging
        .publish(vec![ping("alice", 1), ping("alice", 2)])
        .await
        .unwrap();
    messaging
        .publish(vec![poke("alice", 1), poke("alice", 2)])
        .await
        .unwrap();
    run(&worker).await;

    messaging.later(TimeDelta::days(2));
    worker.compact().await.unwrap();

    assert_eq!(
        messaging.outbox().await,
        [
            "tests.pinged alice v1",
            "tests.pinged alice v2",
            "tests.poked alice v2"
        ]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_topic_not_registered_is_never_compacted(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    let worker = messaging.worker(QUEUE);
    messaging
        .publish(vec![poke("alice", 1), poke("alice", 2)])
        .await
        .unwrap();
    run(&worker).await;

    messaging.later(TimeDelta::days(30));
    worker.compact().await.unwrap();

    assert_eq!(
        messaging.outbox().await,
        ["tests.poked alice v1", "tests.poked alice v2"]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn registering_a_topic_again_takes_its_changed_retention(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    let longer = PublishedTopic {
        name: PingTopic::NAME,
        retention: TimeDelta::days(30),
    };
    messaging.register_topics(&[longer]).await;
    let worker = messaging.worker(QUEUE);
    messaging
        .publish(vec![ping("alice", 1), ping("alice", 2)])
        .await
        .unwrap();
    run(&worker).await;

    messaging.later(TimeDelta::days(8));
    worker.compact().await.unwrap();

    assert_eq!(
        messaging.outbox().await,
        ["tests.pinged alice v1", "tests.pinged alice v2"]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn deliveries_still_to_run_are_untouched_and_still_run(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    let worker = messaging.worker(QUEUE);
    let versions = (1..=3).map(|v| ping("alice", v)).collect();
    messaging.publish(versions).await.unwrap();
    worker.relay().await.unwrap();

    messaging.later(TimeDelta::days(8));
    let deleted = worker.compact().await.unwrap();

    assert_eq!(deleted, 2);
    assert_eq!(messaging.outbox().await, ["tests.pinged alice v3"]);
    assert_eq!(
        messaging.jobs().await,
        [
            Job::untouched("tests.first"),
            Job::untouched("tests.first"),
            Job::untouched("tests.first"),
        ]
    );
    run(&worker).await;
    assert_eq!(
        messaging.log.handled(),
        [
            "tests.first: pinged alice v1",
            "tests.first: pinged alice v2",
            "tests.first: pinged alice v3"
        ]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_dead_letter_is_untouched_and_still_reruns(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    let worker = messaging.worker(QUEUE);
    messaging.fails("alice", 1, 5);
    messaging
        .publish(vec![ping("alice", 1), ping("alice", 2)])
        .await
        .unwrap();
    for _ in 0..5 {
        run(&worker).await;
        messaging.later(TimeDelta::hours(1));
    }
    assert_eq!(worker.dead_letters().await.unwrap().len(), 1);

    messaging.later(TimeDelta::days(8));
    let deleted = worker.compact().await.unwrap();

    assert_eq!(deleted, 1);
    assert_eq!(messaging.outbox().await, ["tests.pinged alice v2"]);
    assert_eq!(worker.dead_letters().await.unwrap().len(), 1);
    messaging.rerun_dead_letters().await;
    run(&worker).await;
    assert_eq!(
        messaging.log.handled(),
        [
            "tests.first: pinged alice v2",
            "tests.first: pinged alice v1"
        ]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_worker_compacts_at_its_first_poll(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    let worker = messaging.worker(QUEUE);
    messaging
        .publish(vec![ping("alice", 1), ping("alice", 2)])
        .await
        .unwrap();
    run(&worker).await;

    messaging.later(TimeDelta::days(8));
    worker.compact_if_due().await.unwrap();

    assert_eq!(messaging.outbox().await, ["tests.pinged alice v2"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_worker_checking_its_clock_doesnt_compact_while_it_is_far_from_the_databases(
    pool: PgPool,
) {
    let messaging = messaging_keeping_pings(pool).await;
    let worker = messaging.worker(QUEUE).checking_its_clock();
    messaging
        .publish(vec![ping("alice", 1), ping("alice", 2)])
        .await
        .unwrap();
    run(&worker).await;

    messaging.later(TimeDelta::days(8));
    assert!(worker.compact_if_due().await.is_err());

    assert_eq!(
        messaging.outbox().await,
        ["tests.pinged alice v1", "tests.pinged alice v2"]
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_worker_compacts_again_only_once_its_period_has_passed(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    let worker = messaging
        .worker(QUEUE)
        .compacting_every(TimeDelta::days(10));
    messaging
        .publish(vec![ping("alice", 1), ping("alice", 2)])
        .await
        .unwrap();
    run(&worker).await;
    messaging.later(TimeDelta::days(8));
    worker.compact_if_due().await.unwrap();
    messaging.publish(vec![ping("alice", 3)]).await.unwrap();
    run(&worker).await;

    messaging.later(TimeDelta::days(8));
    worker.compact_if_due().await.unwrap();
    assert_eq!(
        messaging.outbox().await,
        ["tests.pinged alice v2", "tests.pinged alice v3"]
    );

    messaging.later(TimeDelta::days(2));
    worker.compact_if_due().await.unwrap();
    assert_eq!(messaging.outbox().await, ["tests.pinged alice v3"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_failed_compaction_is_tried_again_only_once_its_period_has_passed(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    let worker = messaging.worker(QUEUE);
    messaging
        .publish(vec![ping("alice", 1), ping("alice", 2)])
        .await
        .unwrap();
    run(&worker).await;
    messaging.later(TimeDelta::days(8));
    messaging.break_compaction().await;
    assert!(worker.compact_if_due().await.is_err());
    messaging.mend_compaction().await;

    messaging.later(TimeDelta::minutes(30));
    worker.compact_if_due().await.unwrap();
    assert_eq!(
        messaging.outbox().await,
        ["tests.pinged alice v1", "tests.pinged alice v2"]
    );

    messaging.later(TimeDelta::minutes(30));
    worker.compact_if_due().await.unwrap();
    assert_eq!(messaging.outbox().await, ["tests.pinged alice v2"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn two_workers_compacting_at_once_leave_the_outbox_one_would(pool: PgPool) {
    let messaging = messaging_keeping_pings(pool).await;
    let keys: Vec<String> = (0..25).map(|k| format!("key-{k}")).collect();
    for key in &keys {
        let versions = (1..=100).map(|v| ping(key, v)).collect();
        messaging.publish(versions).await.unwrap();
    }
    run(&messaging.worker_on(messaging.pool.clone())).await;

    messaging.later(TimeDelta::days(8));
    compact_in_parallel(&messaging, 2);

    let latest: Vec<_> = keys
        .iter()
        .map(|key| format!("tests.pinged {key} v100"))
        .collect();
    assert_eq!(messaging.outbox().await, latest);
}

/// `workers` workers, each on its own thread, runtime and pool, each at its
/// first poll's compaction at once.
fn compact_in_parallel(messaging: &Messaging, workers: usize) {
    let options = messaging.pool.connect_options();
    let barrier = std::sync::Barrier::new(workers);
    std::thread::scope(|threads| {
        let workers: Vec<_> = (0..workers)
            .map(|_| {
                let options = (*options).clone();
                let barrier = &barrier;
                threads.spawn(move || {
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
                        barrier.wait();
                        worker.compact_if_due().await.expect("compact");
                    })
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
    })
}
