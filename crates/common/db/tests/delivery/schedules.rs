use crate::support::{Job, Log, LoggedErrors, Messaging, TEMPLATE, run};
use app_db::{DeadLetter, ScheduleError};
use app_error::{AppError, AppResult};
use app_persistence::{Handlers, MAX_ATTEMPTS, RunScheduled, Schedule, Tx};
use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Notify;

// The test clock starts at 2025-01-01T00:00:00Z.

struct Nightly;

impl Schedule for Nightly {
    const NAME: &'static str = "tests.nightly";
    const CRON: &'static str = "0 3 * * *";
}

/// `Nightly` as a later build declares it, an hour later.
struct NightlyAtFour;

impl Schedule for NightlyAtFour {
    const NAME: &'static str = "tests.nightly";
    const CRON: &'static str = "0 4 * * *";
}

struct Broken;

impl Schedule for Broken {
    const NAME: &'static str = "tests.broken";
    const CRON: &'static str = "0 25 * * *";
}

/// Logs `nightly <slot>`, failing instead while `fails` is set, and, once
/// `holds`, waits for it to be notified after saying it began.
#[derive(Clone)]
struct Nightlies {
    log: Log,
    fails: bool,
    holds: Option<(Arc<Notify>, Arc<Notify>)>,
}

impl Nightlies {
    fn new(messaging: &Messaging) -> Self {
        Self {
            log: messaging.log.clone(),
            fails: false,
            holds: None,
        }
    }
}

#[async_trait]
impl<S: Schedule> RunScheduled<S> for Nightlies {
    async fn run(&self, _: &mut Tx, slot: DateTime<Utc>) -> AppResult<()> {
        if let Some((began, release)) = &self.holds {
            began.notify_one();
            release.notified().await;
        }
        if self.fails {
            return Err(AppError::conflict("not now"));
        }
        self.log.push(format!("nightly {}", slot.to_rfc3339()));
        Ok(())
    }
}

fn nightly(handler: Nightlies) -> Handlers {
    Handlers::new().every::<Nightly>(handler)
}

async fn migrated(messaging: &Messaging, handlers: Handlers) -> app_db::Worker {
    messaging
        .register_schedules(&handlers)
        .await
        .expect("register the schedules");
    messaging.worker_of(handlers)
}

fn at(text: &str) -> DateTime<Utc> {
    text.parse().expect("an RFC 3339 instant")
}

fn cause(err: AppError) -> String {
    match err {
        AppError::Internal(source) => source.to_string(),
        other => other.to_string(),
    }
}

async fn registered(pool: &PgPool) -> Vec<(String, String, DateTime<Utc>)> {
    sqlx::query_as("SELECT name, cron, next_at FROM messaging.schedule ORDER BY name")
        .fetch_all(pool)
        .await
        .expect("read the schedules")
}

#[app_test_db::test(TEMPLATE)]
async fn migrate_registers_a_new_schedule_due_at_its_next_slot_and_keeps_it_when_unchanged(
    pool: PgPool,
) {
    let messaging = Messaging::new(pool).await;
    let handlers = nightly(Nightlies::new(&messaging));

    messaging.register_schedules(&handlers).await.unwrap();
    messaging.later(TimeDelta::hours(1));
    messaging.register_schedules(&handlers).await.unwrap();

    assert_eq!(
        registered(&messaging.pool).await,
        [(
            "tests.nightly".into(),
            "0 3 * * *".into(),
            at("2025-01-01T03:00:00Z")
        )]
    );
}

#[app_test_db::test(TEMPLATE)]
async fn migrate_reschedules_a_schedule_whose_expression_changed(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging
        .register_schedules(&nightly(Nightlies::new(&messaging)))
        .await
        .unwrap();
    messaging.later(TimeDelta::days(1));

    messaging
        .register_schedules(&Handlers::new().every::<NightlyAtFour>(Nightlies::new(&messaging)))
        .await
        .unwrap();

    assert_eq!(
        registered(&messaging.pool).await,
        [(
            "tests.nightly".into(),
            "0 4 * * *".into(),
            at("2025-01-02T04:00:00Z")
        )]
    );
}

#[app_test_db::test(TEMPLATE)]
async fn an_expression_that_does_not_parse_stops_migrate_and_the_worker_naming_the_schedule(
    pool: PgPool,
) {
    let messaging = Messaging::new(pool).await;
    let handlers = || {
        nightly(Nightlies::new(&messaging))
            .merge(Handlers::new().every::<Broken>(Nightlies::new(&messaging)))
    };

    let refused = messaging.register_schedules(&handlers()).await;
    let started = messaging.worker_of(handlers()).drain(None).await;

    assert!(
        matches!(
            &refused,
            Err(ScheduleError::InvalidCron {
                name: "tests.broken",
                ..
            })
        ),
        "{refused:?}"
    );
    assert!(
        refused.unwrap_err().to_string().contains("`tests.broken`"),
        "the refusal names the schedule"
    );
    assert!(registered(&messaging.pool).await.is_empty());
    let refused = cause(started.expect_err("the worker refuses to start"));
    assert!(refused.contains("`tests.broken`"), "{refused}");
}

#[app_test_db::test(TEMPLATE)]
async fn a_worker_refuses_to_start_with_a_schedule_migrate_has_not_registered(pool: PgPool) {
    let messaging = Messaging::new(pool).await;

    let started = messaging
        .worker_of(nightly(Nightlies::new(&messaging)))
        .drain(None)
        .await;

    let refused = cause(started.expect_err("the worker refuses to start"));
    assert!(
        refused.contains("`tests.nightly` isn't registered"),
        "{refused}"
    );
}

#[app_test_db::test(TEMPLATE)]
async fn a_slot_runs_once_its_time_has_come_told_its_slot(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let worker = migrated(&messaging, nightly(Nightlies::new(&messaging))).await;

    messaging.later(TimeDelta::hours(3) - TimeDelta::seconds(1));
    run(&worker).await;
    assert!(messaging.log.handled().is_empty());

    messaging.later(TimeDelta::seconds(1));
    run(&worker).await;
    run(&worker).await;

    assert_eq!(
        messaging.log.handled(),
        ["nightly 2025-01-01T03:00:00+00:00"]
    );
    assert_eq!(
        messaging.next_at("tests.nightly").await,
        at("2025-01-02T03:00:00Z")
    );
    assert!(messaging.jobs().await.is_empty());
}

#[app_test_db::test(TEMPLATE)]
async fn two_workers_create_a_slots_job_once(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let first = migrated(&messaging, nightly(Nightlies::new(&messaging))).await;
    let second = messaging.worker_of(nightly(Nightlies::new(&messaging)));
    messaging.later(TimeDelta::hours(3));

    let (a, b) = tokio::join!(first.schedule(), second.schedule());
    let again = second.schedule().await.unwrap();

    assert_eq!(a.unwrap() + b.unwrap() + again, 1);
    assert_eq!(messaging.jobs().await, [Job::untouched("tests.nightly")]);
    tokio::join!(run(&first), run(&second));
    assert_eq!(
        messaging.log.handled(),
        ["nightly 2025-01-01T03:00:00+00:00"]
    );
}

#[app_test_db::test(TEMPLATE)]
async fn after_downtime_spanning_several_slots_a_schedule_runs_once_for_the_last(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let worker = migrated(&messaging, nightly(Nightlies::new(&messaging))).await;

    messaging.later(TimeDelta::days(3) + TimeDelta::hours(4));
    run(&worker).await;

    assert_eq!(
        messaging.log.handled(),
        ["nightly 2025-01-04T03:00:00+00:00"]
    );
    assert_eq!(
        messaging.next_at("tests.nightly").await,
        at("2025-01-05T03:00:00Z")
    );
    assert!(messaging.next_at("tests.nightly").await > messaging.now());
}

#[app_test_db::test(TEMPLATE)]
async fn a_slot_due_while_the_last_run_is_pending_is_skipped_and_logged(pool: PgPool) {
    let (warnings, _guard) = LoggedErrors::capture_at(tracing::Level::WARN);
    let messaging = Messaging::new(pool).await;
    let worker = migrated(&messaging, nightly(Nightlies::new(&messaging))).await;
    messaging.later(TimeDelta::hours(3));
    assert_eq!(worker.schedule().await.unwrap(), 1);
    messaging.later(TimeDelta::days(1));

    let created = worker.schedule().await.unwrap();

    assert_eq!(created, 0);
    assert_eq!(messaging.jobs().await, [Job::untouched("tests.nightly")]);
    assert_eq!(
        messaging.next_at("tests.nightly").await,
        at("2025-01-03T03:00:00Z")
    );
    assert!(
        skipped(&warnings, "2025-01-02 03:00:00"),
        "{:?}",
        warnings.lines()
    );
}

#[app_test_db::test(TEMPLATE)]
async fn a_slot_due_while_the_last_run_is_running_is_skipped_and_logged(pool: PgPool) {
    // Captured from the start in both skipping tests: a thread with no
    // subscriber reaching the warning first could cache it as never enabled.
    let (warnings, _guard) = LoggedErrors::capture_at(tracing::Level::WARN);
    let messaging = Messaging::new(pool).await;
    let (began, release) = (Arc::new(Notify::new()), Arc::new(Notify::new()));
    let holding = Nightlies {
        holds: Some((began.clone(), release.clone())),
        ..Nightlies::new(&messaging)
    };
    let worker = migrated(&messaging, nightly(holding)).await;
    let other = messaging.worker_of(nightly(Nightlies::new(&messaging)));
    messaging.later(TimeDelta::hours(3));

    let meanwhile = async {
        began.notified().await;
        messaging.later(TimeDelta::days(1));
        let created = other.schedule().await.unwrap();
        release.notify_one();
        created
    };
    let ((), created) = tokio::join!(run(&worker), meanwhile);

    assert_eq!(created, 0);
    assert_eq!(
        messaging.log.handled(),
        ["nightly 2025-01-01T03:00:00+00:00"]
    );
    assert!(messaging.jobs().await.is_empty());
    assert_eq!(
        messaging.next_at("tests.nightly").await,
        at("2025-01-03T03:00:00Z")
    );
    assert!(
        skipped(&warnings, "2025-01-02 03:00:00"),
        "{:?}",
        warnings.lines()
    );
}

/// Whether `tests.nightly`'s slot at `slot` was logged as skipped.
fn skipped(warnings: &LoggedErrors, slot: &str) -> bool {
    warnings.lines().iter().any(|line| {
        line.starts_with("slot skipped") && line.contains("tests.nightly") && line.contains(slot)
    })
}

#[app_test_db::test(TEMPLATE)]
async fn a_failing_run_retries_then_is_dead_lettered_and_the_next_slot_runs_afresh(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let failing = Nightlies {
        fails: true,
        ..Nightlies::new(&messaging)
    };
    let worker = migrated(&messaging, nightly(failing)).await;
    messaging.later(TimeDelta::hours(3));

    for _ in 0..MAX_ATTEMPTS {
        run(&worker).await;
        messaging.later(TimeDelta::hours(1));
    }

    assert_eq!(
        worker.dead_letters().await.unwrap(),
        [DeadLetter {
            queue: "tests.nightly".into(),
            payload: json!({ "slot": "2025-01-01T03:00:00Z" }),
            attempts: MAX_ATTEMPTS,
            last_error: Some("conflict: not now".into()),
        }]
    );
    messaging.later(TimeDelta::days(1));
    worker.schedule().await.unwrap();
    assert_eq!(messaging.jobs().await.len(), 2);
}
