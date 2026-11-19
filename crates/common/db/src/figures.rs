//! The delivery figures read from Postgres (Spec 0003): each queue's jobs,
//! the relay's backlog and each backfill's remainder, exported as gauges by
//! the one worker at a time holding [`FIGURES_LOCK`], and the attempts of
//! each job run, recorded by every worker.

use app_error::{AppError, AppResult};
use app_persistence::MAX_ATTEMPTS;
use chrono::{DateTime, TimeDelta, Utc};
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::{Histogram, Meter};
use sqlx::pool::PoolConnection;
use sqlx::{PgConnection, PgPool, Postgres};
use std::sync::{Arc, Mutex};

/// The session advisory lock the worker reading the figures holds for as long
/// as its connection lives: `"figures"` in ASCII.
const FIGURES_LOCK: i64 = 0x66_6967_7572_6573;

/// What the queues, the relay and the backfills hold, as one worker read it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Figures {
    /// Every queue subscribed or holding a job, by name.
    pub queues: Vec<QueueFigures>,
    pub relay: RelayFigures,
    /// Every backfill, a finished one's with nothing left.
    pub backfills: Vec<BackfillFigures>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueueFigures {
    pub queue: String,
    /// Live jobs whose `run_at` has come.
    pub due: i64,
    /// Live jobs whose `run_at` is later: backing off, or running, since a
    /// claim pushes `run_at` past its lease.
    pub waiting: i64,
    /// Since the `run_at` of the earliest due job; zero with none due.
    pub oldest_due_age: TimeDelta,
    pub dead_letters: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RelayFigures {
    pub undispatched: i64,
    /// Since the earliest undispatched event occurred; zero with none.
    pub oldest_age: TimeDelta,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackfillFigures {
    pub queue: String,
    pub event_type: String,
    /// Events still to copy into the queue.
    pub remaining: i64,
}

/// What a worker last read, while it holds the lock: the gauges' callbacks
/// observe it at each collection, and nothing while it is `None`.
type Snapshot = Arc<Mutex<Option<Figures>>>;

pub(crate) struct FiguresReader {
    pool: PgPool,
    /// Open only while it holds [`FIGURES_LOCK`].
    holding: tokio::sync::Mutex<Option<PgConnection>>,
    snapshot: Snapshot,
}

impl FiguresReader {
    pub(crate) fn new(pool: PgPool) -> Self {
        let meter = meter();
        let snapshot = Snapshot::default();
        let by_queue = |value: fn(&QueueFigures) -> i64| {
            move |f: &Figures| -> Points<i64> {
                f.queues
                    .iter()
                    .map(|q| (value(q), queue_attributes(q)))
                    .collect()
            }
        };
        count(
            &meter,
            &snapshot,
            "messaging.queue.jobs.due",
            "{job}",
            "Live jobs in the queue whose run_at has come.",
            by_queue(|q| q.due),
        );
        count(
            &meter,
            &snapshot,
            "messaging.queue.jobs.waiting",
            "{job}",
            "Live jobs in the queue waiting for a later run_at: backing off or running.",
            by_queue(|q| q.waiting),
        );
        count(
            &meter,
            &snapshot,
            "messaging.queue.dead_letters",
            "{job}",
            "Jobs in the queue that ran out of attempts.",
            by_queue(|q| q.dead_letters),
        );
        count(
            &meter,
            &snapshot,
            "messaging.relay.undispatched",
            "{event}",
            "Outbox events not yet relayed to their queues.",
            |f| vec![(f.relay.undispatched, vec![])],
        );
        count(
            &meter,
            &snapshot,
            "messaging.backfill.remaining",
            "{event}",
            "Events a subscription's backfill has still to copy into its queue.",
            |f| {
                f.backfills
                    .iter()
                    .map(|b| {
                        let attributes = vec![
                            KeyValue::new("messaging.destination.name", b.queue.clone()),
                            KeyValue::new("event_type", b.event_type.clone()),
                        ];
                        (b.remaining, attributes)
                    })
                    .collect()
            },
        );
        age(
            &meter,
            &snapshot,
            "messaging.queue.oldest_due.age",
            "How long the queue's earliest due job has been due.",
            |f| {
                f.queues
                    .iter()
                    .map(|q| (q.oldest_due_age, queue_attributes(q)))
                    .collect()
            },
        );
        age(
            &meter,
            &snapshot,
            "messaging.relay.oldest_undispatched.age",
            "How long ago the earliest outbox event not yet relayed occurred.",
            |f| vec![(f.relay.oldest_age, vec![])],
        );
        Self {
            pool,
            holding: tokio::sync::Mutex::new(None),
            snapshot,
        }
    }

    /// `None` while another worker holds the lock. A failure lets the lock
    /// go, so another worker may take over, and leaves nothing observed.
    pub(crate) async fn read(&self, now: DateTime<Utc>) -> AppResult<Option<Figures>> {
        let mut holding = self.holding.lock().await;
        if holding.is_none() {
            *holding = self.lock().await?;
        }
        let Some(conn) = holding.as_mut() else {
            return Ok(None);
        };
        match read(conn, now).await {
            Ok(figures) => {
                *self.snapshot.lock().expect("not poisoned") = Some(figures.clone());
                Ok(Some(figures))
            }
            Err(err) => {
                *holding = None;
                *self.snapshot.lock().expect("not poisoned") = None;
                Err(err)
            }
        }
    }

    /// Tries on a pooled connection, detached from the pool only once it
    /// holds the lock, so the workers that don't keep no connection for it.
    async fn lock(&self) -> AppResult<Option<PgConnection>> {
        let mut conn: PoolConnection<Postgres> =
            self.pool.acquire().await.map_err(AppError::internal)?;
        let locked = sqlx::query_scalar!(
            r#"SELECT pg_try_advisory_lock($1) AS "locked!""#,
            FIGURES_LOCK,
        )
        .fetch_one(&mut *conn)
        .await
        .map_err(AppError::internal)?;
        Ok(locked.then(|| conn.detach()))
    }
}

/// The gauges' callbacks outlive the reader, for as long as the meter
/// provider does, so a dropped reader leaves them nothing to observe.
impl Drop for FiguresReader {
    fn drop(&mut self) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            *snapshot = None;
        }
    }
}

async fn read(conn: &mut PgConnection, now: DateTime<Utc>) -> AppResult<Figures> {
    let queues = sqlx::query!(
        r#"
        SELECT q.queue AS "queue!",
               count(j.id) FILTER (WHERE j.dead_at IS NULL AND j.run_at <= $1) AS "due!",
               count(j.id) FILTER (WHERE j.dead_at IS NULL AND j.run_at > $1) AS "waiting!",
               min(j.run_at) FILTER (WHERE j.dead_at IS NULL AND j.run_at <= $1) AS oldest_due,
               count(j.id) FILTER (WHERE j.dead_at IS NOT NULL) AS "dead_letters!"
        FROM (
            SELECT queue FROM messaging.subscription
            UNION SELECT DISTINCT queue FROM messaging.job
        ) q
        LEFT JOIN messaging.job j ON j.queue = q.queue
        GROUP BY q.queue
        ORDER BY q.queue
        "#,
        now,
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?
    .into_iter()
    .map(|q| QueueFigures {
        queue: q.queue,
        due: q.due,
        waiting: q.waiting,
        oldest_due_age: since(q.oldest_due, now),
        dead_letters: q.dead_letters,
    })
    .collect();
    let relay = sqlx::query!(
        r#"
        SELECT count(*) AS "undispatched!", min(occurred_at) AS oldest
        FROM messaging.outbox
        WHERE dispatched_at IS NULL
        "#,
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let backfills = sqlx::query_as!(
        BackfillFigures,
        r#"
        SELECT b.queue, b.event_type, count(o.seq) AS "remaining!"
        FROM messaging.backfill b
        LEFT JOIN messaging.outbox o
          ON b.completed_at IS NULL AND o.topic = b.topic AND o.event_type = b.event_type
         AND o.seq > b.after_seq AND o.relay_batch <= b.through_batch
        GROUP BY b.queue, b.event_type
        ORDER BY b.queue, b.event_type
        "#,
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    Ok(Figures {
        queues,
        relay: RelayFigures {
            undispatched: relay.undispatched,
            oldest_age: since(relay.oldest, now),
        },
        backfills,
    })
}

fn since(earliest: Option<DateTime<Utc>>, now: DateTime<Utc>) -> TimeDelta {
    earliest.map_or(TimeDelta::zero(), |at| (now - at).max(TimeDelta::zero()))
}

fn meter() -> Meter {
    global::meter("app-db")
}

type Points<T> = Vec<(T, Vec<KeyValue>)>;

fn count(
    meter: &Meter,
    snapshot: &Snapshot,
    name: &'static str,
    unit: &'static str,
    description: &'static str,
    points: impl Fn(&Figures) -> Points<i64> + Send + Sync + 'static,
) {
    let snapshot = snapshot.clone();
    meter
        .i64_observable_gauge(name)
        .with_unit(unit)
        .with_description(description)
        .with_callback(move |observer| {
            if let Some(figures) = snapshot.lock().expect("not poisoned").as_ref() {
                for (value, attributes) in points(figures) {
                    observer.observe(value, &attributes);
                }
            }
        })
        .build();
}

fn age(
    meter: &Meter,
    snapshot: &Snapshot,
    name: &'static str,
    description: &'static str,
    points: impl Fn(&Figures) -> Points<TimeDelta> + Send + Sync + 'static,
) {
    let snapshot = snapshot.clone();
    meter
        .f64_observable_gauge(name)
        .with_unit("s")
        .with_description(description)
        .with_callback(move |observer| {
            if let Some(figures) = snapshot.lock().expect("not poisoned").as_ref() {
                for (age, attributes) in points(figures) {
                    observer.observe(seconds(age), &attributes);
                }
            }
        })
        .build();
}

fn seconds(age: TimeDelta) -> f64 {
    age.num_milliseconds() as f64 / 1000.0
}

fn queue_attributes(queue: &QueueFigures) -> Vec<KeyValue> {
    vec![KeyValue::new(
        "messaging.destination.name",
        queue.queue.clone(),
    )]
}

/// `messaging.job.attempts`: the attempt each run of a job's handler is, by
/// queue; a claim past its last attempt is abandoned unrun, and not recorded.
#[derive(Clone)]
pub(crate) struct Attempts(Histogram<u64>);

impl Attempts {
    pub(crate) fn new() -> Self {
        Self(
            meter()
                .u64_histogram("messaging.job.attempts")
                .with_unit("{attempt}")
                .with_description(
                    "Which attempt each run of a job's handler is, the first being 1.",
                )
                .with_boundaries((1..=MAX_ATTEMPTS).map(f64::from).collect())
                .build(),
        )
    }

    pub(crate) fn record(&self, queue: &str, attempt: i32) {
        self.0.record(
            u64::try_from(attempt).unwrap_or(0),
            &[KeyValue::new(
                "messaging.destination.name",
                queue.to_owned(),
            )],
        );
    }
}
