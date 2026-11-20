use crate::support::{LoggedErrors, Messaging, PingTopic, run};
use app_db::{QueueFigures, RelayFigures};
use app_persistence::{Handlers, MAX_ATTEMPTS};
use app_web::testing::Metrics;
use chrono::TimeDelta;
use sqlx::PgPool;
use std::time::Duration;

const QUEUE: &[&str] = &["tests.first"];
const COPY: &str = "tests.copy";

/// After this, `tests.first` holds a dead letter (dave), a job backing off
/// (carol) and a due one (bob), `tests.copy` is to be backfilled with those
/// three pings, and erin's ping is unrelayed: bob's and erin's five seconds old,
/// carol's backoff five seconds from over.
async fn seeded(messaging: &Messaging) {
    messaging.migrate(QUEUE).await;
    let worker = messaging.worker(QUEUE);
    messaging.fails("dave", 1, MAX_ATTEMPTS as u32);
    messaging.ping("dave").await;
    run(&worker).await;
    for _ in 1..MAX_ATTEMPTS {
        messaging.later(TimeDelta::hours(1));
        run(&worker).await;
    }
    messaging.fails("carol", 1, 1);
    messaging.ping("carol").await;
    run(&worker).await;
    messaging.ping("bob").await;
    worker.relay().await.unwrap();
    messaging
        .register(&Handlers::new().on_from_earliest::<PingTopic>(COPY, messaging.recorder(COPY)))
        .await
        .unwrap();
    messaging.ping("erin").await;
    messaging.later(TimeDelta::seconds(5));
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn the_figures_reflect_what_the_queues_the_relay_and_the_backfills_hold(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    seeded(&messaging).await;

    let figures = messaging
        .worker(QUEUE)
        .read_figures()
        .await
        .unwrap()
        .expect("the only worker reads the figures");

    assert_eq!(
        figures.queues,
        [
            QueueFigures {
                queue: COPY.into(),
                due: 0,
                waiting: 0,
                oldest_due_age: TimeDelta::zero(),
                dead_letters: 0,
            },
            QueueFigures {
                queue: "tests.first".into(),
                due: 1,
                waiting: 1,
                oldest_due_age: TimeDelta::seconds(5),
                dead_letters: 1,
            },
        ]
    );
    assert_eq!(
        figures.relay,
        RelayFigures {
            undispatched: 1,
            oldest_age: TimeDelta::seconds(5),
        }
    );
    let pinged = figures
        .backfills
        .iter()
        .find(|b| b.queue == COPY && b.event_type == "tests.pinged")
        .expect("the pings' backfill");
    assert_eq!(pinged.remaining, 3);
    assert!(
        figures
            .backfills
            .iter()
            .filter(|b| b.event_type != "tests.pinged")
            .all(|b| b.remaining == 0)
    );
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_backfill_copied_has_nothing_left(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    seeded(&messaging).await;
    let worker = messaging.worker(QUEUE);
    worker.backfill().await.unwrap();

    let figures = worker.read_figures().await.unwrap().unwrap();

    assert!(figures.backfills.iter().all(|b| b.remaining == 0));
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn only_one_worker_reads_the_figures_until_it_stops(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    let first = messaging.worker(QUEUE);
    let second = messaging.worker(QUEUE);

    assert!(first.read_figures().await.unwrap().is_some());
    assert!(second.read_figures().await.unwrap().is_none());
    assert!(first.read_figures().await.unwrap().is_some());

    drop(first);
    // The lock goes once Postgres notices the connection close.
    let mut tries = 0;
    while second.read_figures().await.unwrap().is_none() {
        tries += 1;
        assert!(tries < 100, "the second worker never took over");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_failure_to_read_the_figures_is_logged_and_the_worker_runs_on(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    let worker = messaging.worker(QUEUE);
    messaging.ping("alice").await;
    rename_backfills(&messaging.pool, "backfill", "backfill_broken").await;
    let (errors, _capturing) = LoggedErrors::capture();

    let mended_and_handled = async {
        while !errors
            .lines()
            .iter()
            .any(|line| line.starts_with("reading the delivery figures failed"))
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        rename_backfills(&messaging.pool, "backfill_broken", "backfill").await;
        while messaging.log.handled().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(
        Duration::from_secs(10),
        worker.run(Duration::from_millis(10), mended_and_handled),
    )
    .await
    .expect("the worker runs on")
    .unwrap();

    assert_eq!(messaging.log.handled(), ["tests.first: pinged alice v1"]);
}

async fn rename_backfills(pool: &PgPool, from: &str, to: &str) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER TABLE messaging.{from} RENAME TO {to}"
    )))
    .execute(pool)
    .await
    .unwrap();
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn every_delivery_metric_is_registered(pool: PgPool) {
    let metrics = Metrics::install();
    let messaging = Messaging::new(pool).await;
    seeded(&messaging).await;

    let worker = messaging.worker(QUEUE);
    worker.read_figures().await.unwrap();

    let names = metrics.names();
    for metric in [
        "messaging.queue.jobs.due",
        "messaging.queue.jobs.waiting",
        "messaging.queue.oldest_due.age",
        "messaging.queue.dead_letters",
        "messaging.job.attempts",
        "messaging.relay.undispatched",
        "messaging.relay.oldest_undispatched.age",
        "messaging.backfill.remaining",
    ] {
        assert!(names.contains(metric), "{metric} isn't recorded: {names:?}");
    }
}
