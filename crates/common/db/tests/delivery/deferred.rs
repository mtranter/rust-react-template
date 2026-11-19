use crate::support::{Log, Messaging, TEMPLATE, run};
use app_db::{DeadLetter, conn};
use app_error::{AppError, AppResult};
use app_persistence::{DeferredJob, Handlers, MAX_ATTEMPTS, Run, Tx};
use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;

#[derive(Debug, Serialize, Deserialize)]
struct Remind {
    who: String,
}

impl DeferredJob for Remind {
    const NAME: &'static str = "tests.remind";
}

fn remind(who: &str) -> Remind {
    Remind { who: who.into() }
}

/// Logs `remind <who>`, failing instead while `fails` is set.
struct Reminder {
    log: Log,
    fails: bool,
}

#[async_trait]
impl Run<Remind> for Reminder {
    async fn run(&self, _: &mut Tx, job: Remind) -> AppResult<()> {
        if self.fails {
            return Err(AppError::conflict("not now"));
        }
        self.log.push(format!("remind {}", job.who));
        Ok(())
    }
}

fn worker(messaging: &Messaging, fails: bool) -> app_db::Worker {
    let reminder = Reminder {
        log: messaging.log.clone(),
        fails,
    };
    messaging.worker_of(Handlers::new().runs::<Remind>(reminder))
}

async fn defer(messaging: &Messaging, key: &str, at: DateTime<Utc>, job: Remind) {
    let mut tx = messaging.begin().await;
    messaging
        .deferred_jobs()
        .defer(&mut tx, key, at, &job)
        .await
        .expect("defer");
    tx.commit().await.expect("commit");
}

async fn cancel(messaging: &Messaging, key: &str) -> AppResult<()> {
    let mut tx = messaging.begin().await;
    messaging
        .deferred_jobs()
        .cancel::<Remind>(&mut tx, key)
        .await?;
    tx.commit().await
}

/// Every pending (not dead) deferred job: `<key> <payload> at <run_at>`.
async fn pending(pool: &PgPool) -> Vec<String> {
    let rows: Vec<(String, serde_json::Value, DateTime<Utc>)> = sqlx::query_as(
        "SELECT key, payload, run_at FROM messaging.job \
         WHERE deferred AND dead_at IS NULL ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .expect("read the jobs");
    rows.into_iter()
        .map(|(key, payload, at)| format!("{key} {payload} at {}", at.to_rfc3339()))
        .collect()
}

fn at(messaging: &Messaging, by: TimeDelta) -> DateTime<Utc> {
    messaging.now() + by
}

#[app_test_db::test(TEMPLATE)]
async fn a_deferred_job_runs_only_once_its_time_has_come(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let worker = worker(&messaging, false);
    defer(
        &messaging,
        "alice",
        at(&messaging, TimeDelta::hours(1)),
        remind("alice"),
    )
    .await;

    run(&worker).await;
    assert!(messaging.log.handled().is_empty());

    messaging.later(TimeDelta::hours(1));
    run(&worker).await;
    run(&worker).await;

    assert_eq!(messaging.log.handled(), ["remind alice"]);
    assert!(pending(&messaging.pool).await.is_empty());
}

#[app_test_db::test(TEMPLATE)]
async fn a_job_deferred_by_a_change_that_rolls_back_is_never_written(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let mut tx = messaging.begin().await;

    messaging
        .deferred_jobs()
        .defer(&mut tx, "alice", messaging.now(), &remind("alice"))
        .await
        .unwrap();
    drop(tx);

    assert!(pending(&messaging.pool).await.is_empty());
}

#[app_test_db::test(TEMPLATE)]
async fn deferring_again_with_the_same_key_replaces_the_pending_jobs_time_and_payload(
    pool: PgPool,
) {
    let messaging = Messaging::new(pool).await;
    let worker = worker(&messaging, false);
    let first = at(&messaging, TimeDelta::hours(1));
    let second = at(&messaging, TimeDelta::hours(2));

    defer(&messaging, "alice", first, remind("alice")).await;
    defer(&messaging, "alice", second, remind("alice, later")).await;

    assert_eq!(
        pending(&messaging.pool).await,
        [format!(
            "alice {} at {}",
            json!({ "who": "alice, later" }),
            second.to_rfc3339()
        )]
    );
    messaging.later(TimeDelta::hours(1));
    run(&worker).await;
    assert!(messaging.log.handled().is_empty());
    messaging.later(TimeDelta::hours(1));
    run(&worker).await;
    assert_eq!(messaging.log.handled(), ["remind alice, later"]);
}

#[app_test_db::test(TEMPLATE)]
async fn jobs_of_different_keys_are_deferred_apart(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let worker = worker(&messaging, false);

    defer(&messaging, "alice", messaging.now(), remind("alice")).await;
    defer(&messaging, "bob", messaging.now(), remind("bob")).await;
    run(&worker).await;

    assert_eq!(messaging.log.handled(), ["remind alice", "remind bob"]);
}

#[app_test_db::test(TEMPLATE)]
async fn cancelling_removes_the_pending_job(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let worker = worker(&messaging, false);
    defer(
        &messaging,
        "alice",
        at(&messaging, TimeDelta::hours(1)),
        remind("alice"),
    )
    .await;
    defer(
        &messaging,
        "bob",
        at(&messaging, TimeDelta::hours(1)),
        remind("bob"),
    )
    .await;

    cancel(&messaging, "alice").await.unwrap();
    messaging.later(TimeDelta::hours(1));
    run(&worker).await;

    assert_eq!(messaging.log.handled(), ["remind bob"]);
}

#[app_test_db::test(TEMPLATE)]
async fn cancelling_a_job_that_already_ran_or_never_existed_is_no_error(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let worker = worker(&messaging, false);
    defer(&messaging, "alice", messaging.now(), remind("alice")).await;
    run(&worker).await;

    assert!(cancel(&messaging, "alice").await.is_ok());
    assert!(cancel(&messaging, "nobody").await.is_ok());
    assert_eq!(messaging.log.handled(), ["remind alice"]);
}

#[app_test_db::test(TEMPLATE)]
async fn a_failing_deferred_job_retries_then_is_dead_lettered(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let worker = worker(&messaging, true);
    defer(&messaging, "alice", messaging.now(), remind("alice")).await;

    for _ in 0..MAX_ATTEMPTS {
        run(&worker).await;
        messaging.later(TimeDelta::hours(1));
    }

    assert_eq!(
        worker.dead_letters().await.unwrap(),
        [DeadLetter {
            queue: Remind::NAME.into(),
            payload: json!({ "who": "alice" }),
            attempts: MAX_ATTEMPTS,
            last_error: Some("conflict: not now".into()),
        }]
    );
    assert!(pending(&messaging.pool).await.is_empty());
}

#[app_test_db::test(TEMPLATE)]
async fn a_dead_letter_steps_aside_for_the_next_job_of_its_key(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let failing = worker(&messaging, true);
    defer(&messaging, "alice", messaging.now(), remind("alice")).await;
    for _ in 0..MAX_ATTEMPTS {
        run(&failing).await;
        messaging.later(TimeDelta::hours(1));
    }

    defer(&messaging, "alice", messaging.now(), remind("alice again")).await;
    run(&worker(&messaging, false)).await;

    assert_eq!(messaging.log.handled(), ["remind alice again"]);
    assert_eq!(failing.dead_letters().await.unwrap().len(), 1);
}

/// Reschedules its own job from another connection while it runs, as a use
/// case changing the time would, then writes its effect.
struct RescheduledWhileRunning {
    messaging_pool: PgPool,
    to: DateTime<Utc>,
    log: Log,
}

#[async_trait]
impl Run<Remind> for RescheduledWhileRunning {
    async fn run(&self, tx: &mut Tx, job: Remind) -> AppResult<()> {
        if job.who == "alice" {
            sqlx::query(
                "UPDATE messaging.job SET run_at = $1, version = version + 1, \
                 payload = '{\"who\": \"alice, later\"}' WHERE key = 'alice'",
            )
            .bind(self.to)
            .execute(&self.messaging_pool)
            .await
            .map_err(AppError::internal)?;
        }
        sqlx::query("INSERT INTO tests_effect (who) VALUES ($1)")
            .bind(&job.who)
            .execute(conn(tx))
            .await
            .map_err(AppError::internal)?;
        self.log.push(format!("remind {}", job.who));
        Ok(())
    }
}

#[app_test_db::test(TEMPLATE)]
async fn a_run_rescheduled_while_under_way_takes_no_effect_and_the_new_one_runs(pool: PgPool) {
    sqlx::query("CREATE TABLE tests_effect (who text NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    let messaging = Messaging::new(pool).await;
    let later = at(&messaging, TimeDelta::hours(1));
    let worker = messaging.worker_of(Handlers::new().runs::<Remind>(RescheduledWhileRunning {
        messaging_pool: messaging.pool.clone(),
        to: later,
        log: messaging.log.clone(),
    }));
    defer(&messaging, "alice", messaging.now(), remind("alice")).await;

    run(&worker).await;
    let effects = || async {
        sqlx::query_scalar::<_, String>("SELECT who FROM tests_effect")
            .fetch_all(&messaging.pool)
            .await
            .unwrap()
    };
    assert!(effects().await.is_empty());

    messaging.later(TimeDelta::hours(1));
    run(&worker).await;
    assert_eq!(effects().await, ["alice, later"]);
    assert!(pending(&messaging.pool).await.is_empty());
}
