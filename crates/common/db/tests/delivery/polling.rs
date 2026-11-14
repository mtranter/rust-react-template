use crate::support::{Messaging, PingTopic, ping, run};
use app_events::PublishedTopic;
use chrono::TimeDelta;
use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::oneshot;

const QUEUE: &[&str] = &["tests.first"];
const POLL: Duration = Duration::from_millis(50);
const SOON: Duration = Duration::from_secs(10);

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_running_worker_handles_what_is_published_at_its_next_poll(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    let worker = messaging.worker(QUEUE);
    let (stop, stopped) = oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        worker
            .run(POLL, async {
                stopped.await.ok();
            })
            .await
    });

    for name in ["alice", "bob"] {
        messaging.ping(name).await;
        tokio::time::timeout(SOON, async {
            while !messaging
                .log
                .handled()
                .contains(&format!("tests.first: pinged {name} v1"))
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("handled at a poll");
    }

    stop.send(()).expect("the worker is running");
    running.await.unwrap().expect("the worker ran");
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_running_worker_compacts_at_its_first_poll(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    messaging
        .register_topics(&[PublishedTopic::of::<PingTopic>()])
        .await;
    messaging
        .publish(vec![ping("alice", 1), ping("alice", 2)])
        .await
        .unwrap();
    run(&messaging.worker(QUEUE)).await;
    messaging.later(TimeDelta::days(8));

    let worker = messaging.worker(QUEUE);
    let (stop, stopped) = oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        worker
            .run(POLL, async {
                stopped.await.ok();
            })
            .await
    });

    tokio::time::timeout(SOON, async {
        while messaging.outbox().await != ["tests.pinged alice v2"] {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("compacted at its first poll");

    stop.send(()).expect("the worker is running");
    running.await.unwrap().expect("the worker ran");
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_running_worker_whose_compaction_fails_still_relays_and_runs_jobs(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    messaging
        .register_topics(&[PublishedTopic::of::<PingTopic>()])
        .await;
    messaging.break_compaction().await;
    let worker = messaging.worker(QUEUE);
    let (stop, stopped) = oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        worker
            .run(POLL, async {
                stopped.await.ok();
            })
            .await
    });

    for name in ["alice", "bob"] {
        messaging.ping(name).await;
        tokio::time::timeout(SOON, async {
            while !messaging
                .log
                .handled()
                .contains(&format!("tests.first: pinged {name} v1"))
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("handled at a poll");
    }

    stop.send(()).expect("the worker is running");
    running.await.unwrap().expect("the worker ran");
}
