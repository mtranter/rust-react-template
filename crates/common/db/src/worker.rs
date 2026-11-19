//! Delivers what units of work leave in `messaging` (ADR 0013): the relay copies
//! each outbox event into one delivery per subscriber, a row of `messaging.job`,
//! then each runs in its own transaction, with retries and dead-lettering, in
//! order per key within each queue. Any number of workers can run against one
//! database: one relays at a time, every claim skips rows another worker holds,
//! and a worker claims only the subscriptions its own handlers can decode, so
//! builds can differ during a rolling deploy. Workers also copy each new
//! `earliest` subscription's backfill into its queue, a chunk at a time, and
//! turn each schedule's slot that falls due into a job.

use crate::cron::Cron;
use crate::schedules::parse_schedules;
use crate::unit_of_work::{begin, conn};
use app_domain::Clock;
use app_error::{AppError, AppResult};
use app_events::{EnvelopeOf, Event, Topic};
use app_persistence::{Handlers, MAX_ATTEMPTS, Slot};
use chrono::{DateTime, TimeDelta, Utc};
use serde_json::Value;
use sqlx::PgPool;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::field::Empty;
use tracing::{Instrument, Span};

/// How long a claimed job is hidden from other workers. A handler that runs
/// longer may run twice at once, but only one run's database writes commit;
/// its effects outside the database, such as mail, can happen twice, which is
/// one reason subscribers stay idempotent. One whose worker dies runs again
/// after it. Each worker judges it by its own clock, so workers' clocks must
/// agree to well within it: the `worker` binary refuses to start, and skips
/// compaction, while its clock is more than [`CLOCK_REFUSED`] from the
/// database's ([`Worker::check_clock`]).
const LEASE: TimeDelta = TimeDelta::minutes(5);
/// Two workers each within this of the database's clock are within twice it
/// of each other, well inside the [`LEASE`].
const CLOCK_REFUSED: TimeDelta = TimeDelta::minutes(1);
/// Seconds, not milliseconds: the query's round trip adds to the measured
/// difference.
const CLOCK_WARNED: TimeDelta = TimeDelta::seconds(5);
const RELAY_BATCH: i64 = 100;
/// Each its own transaction, so a compaction never holds the outbox for long.
const COMPACT_BATCH: i64 = 1000;
/// The advisory lock only one relay holds at a time: `"relay"` in ASCII.
/// `messaging.retire` (migration 0009) takes it too, as 491327873401.
pub(crate) const RELAY_LOCK: i64 = 0x72_656c_6179;
const UNFINISHED: &str =
    "no attempt finished: its worker stopped or outran the lease on the last one";

pub struct Worker {
    pool: PgPool,
    clock: Arc<dyn Clock>,
    handlers: Arc<Handlers>,
    compact_every: TimeDelta,
    backfill_chunk: i64,
    checks_clock_before_compacting: bool,
    /// By the worker's clock; in memory only, so a restarted worker compacts
    /// at its first poll.
    last_compacted: Mutex<Option<DateTime<Utc>>>,
}

/// A delivery that ran out of attempts; the later versions of its key run past
/// it. Rerun one by clearing its `dead_at` and `attempts` in `messaging.job`:
/// it then runs after any later versions already handled, out of order, which
/// is why a subscriber keeping a copy of state compares versions (ADR 0013).
#[derive(Debug, Clone, PartialEq)]
pub struct DeadLetter {
    pub queue: String,
    pub payload: Value,
    pub attempts: i32,
    pub last_error: Option<String>,
}

struct Claimed {
    id: i64,
    queue: String,
    event_type: String,
    key: String,
    version: i64,
    payload: Value,
    attempts: i32,
    max_attempts: i32,
}

impl Worker {
    pub fn new(pool: PgPool, clock: Arc<dyn Clock>, handlers: Handlers) -> Self {
        Self {
            pool,
            clock,
            handlers: Arc::new(handlers),
            compact_every: Self::COMPACT_EVERY,
            backfill_chunk: Self::BACKFILL_CHUNK,
            checks_clock_before_compacting: false,
            last_compacted: Mutex::new(None),
        }
    }

    /// [`compact_if_due`](Self::compact_if_due) refuses to compact while
    /// [`check_clock`](Self::check_clock) fails, since compaction can't be
    /// undone. Off unless asked for: a test driving time itself keeps a clock
    /// far from the database's.
    pub fn checking_its_clock(self) -> Self {
        Self {
            checks_clock_before_compacting: true,
            ..self
        }
    }

    /// How often a worker compacts unless told otherwise.
    pub const COMPACT_EVERY: TimeDelta = TimeDelta::hours(1);

    /// How often [`compact_if_due`](Self::compact_if_due) compacts.
    pub fn compacting_every(self, every: TimeDelta) -> Self {
        Self {
            compact_every: every,
            ..self
        }
    }

    /// How many events a worker copies per transaction of a backfill unless
    /// told otherwise: each chunk is its own transaction, so a backfill never
    /// holds anything for long.
    pub const BACKFILL_CHUNK: i64 = 1000;

    /// How many events each transaction of [`backfill`](Self::backfill) copies.
    pub fn backfilling_in_chunks_of(self, events: i64) -> Self {
        assert!(events > 0, "a backfill chunk copies at least one event");
        Self {
            backfill_chunk: events,
            ..self
        }
    }

    /// Polls every `interval` until `shutdown` resolves, compacting the
    /// outbox after each poll that it's due. Refuses to start unless
    /// [`check_subscriptions`](Self::check_subscriptions) and
    /// [`check_schedules`](Self::check_schedules) pass; after that, a failure
    /// to reach the database is logged and retried at the next poll.
    pub async fn run(
        &self,
        interval: Duration,
        shutdown: impl Future<Output = ()>,
    ) -> AppResult<()> {
        self.check_subscriptions().await?;
        self.check_schedules().await?;
        tokio::pin!(shutdown);
        loop {
            if let Err(err) = self.run_until_idle().await {
                tracing::error!(error = %describe(&err), "worker poll failed");
            }
            if let Err(err) = self.compact_if_due().await {
                tracing::error!(error = %describe(&err), "outbox compaction failed");
            }
            tokio::select! {
                _ = &mut shutdown => return Ok(()),
                _ = tokio::time::sleep(interval) => {}
            }
        }
    }

    /// [`run`](Self::run)'s poll, once: relays, backfills and runs until
    /// nothing is due by the worker's clock, compacts if due, and returns, so
    /// an external timer can drive the worker instead. With a `budget`, it
    /// returns once that is spent, dropping what is in flight as a stopped
    /// worker would: a handler's writes roll back and its job runs again after
    /// its lease. Unlike `run`, a failure to reach the database is returned;
    /// failed jobs are retried and dead-lettered as usual, and aren't.
    pub async fn drain(&self, budget: Option<Duration>) -> AppResult<()> {
        self.check_subscriptions().await?;
        self.check_schedules().await?;
        let pass = async {
            self.run_until_idle().await?;
            if let Err(err) = self.compact_if_due().await {
                tracing::error!(error = %describe(&err), "outbox compaction failed");
            }
            Ok(())
        };
        let Some(budget) = budget else {
            return pass.await;
        };
        tokio::time::timeout(budget, pass)
            .await
            .unwrap_or_else(|_| {
                tracing::info!(?budget, "drain stopped: its budget is spent");
                Ok(())
            })
    }

    /// Fails unless `migrate` registered every subscription this worker serves,
    /// under the same topic; one retired since still runs its pending jobs.
    /// Warns of each live subscription it doesn't serve: one another build's
    /// workers must run.
    pub async fn check_subscriptions(&self) -> AppResult<()> {
        let registered = sqlx::query!(
            r#"
            SELECT queue, event_type, topic, retired_at IS NOT NULL AS "retired!"
            FROM messaging.subscription
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::internal)?;
        let own: Vec<_> = self.handlers.subscriptions().collect();
        for s in &own {
            let found = registered
                .iter()
                .find(|r| r.queue == s.queue && r.event_type == s.event_type);
            match found {
                None => {
                    return Err(AppError::internal(Unregistered {
                        queue: s.queue,
                        event_type: s.event_type,
                    }));
                }
                Some(r) if r.topic != s.topic => {
                    return Err(AppError::internal(OtherTopic {
                        queue: s.queue,
                        registered: r.topic.clone(),
                        served: s.topic,
                    }));
                }
                Some(_) => {}
            }
        }
        for r in registered.iter().filter(|r| !r.retired) {
            if !own
                .iter()
                .any(|s| s.queue == r.queue && s.event_type == r.event_type)
            {
                tracing::warn!(
                    queue = r.queue,
                    event_type = r.event_type,
                    "a live subscription this worker doesn't serve"
                );
            }
        }
        Ok(())
    }

    /// Fails on a schedule this worker runs whose expression doesn't parse,
    /// naming it, or that `migrate` hasn't registered.
    pub async fn check_schedules(&self) -> AppResult<()> {
        let own = parse_schedules(&self.handlers).map_err(AppError::internal)?;
        let registered = sqlx::query_scalar!("SELECT name FROM messaging.schedule")
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;
        match own
            .iter()
            .find(|(s, _)| !registered.iter().any(|r| r == s.name))
        {
            Some((s, _)) => Err(AppError::internal(UnregisteredSchedule(s.name))),
            None => Ok(()),
        }
    }

    /// Fails when this worker's clock is more than [`CLOCK_REFUSED`] from the
    /// database's, and warns past [`CLOCK_WARNED`]. Not part of
    /// [`run`](Self::run): a test driving time itself must be able to skip it.
    pub async fn check_clock(&self) -> AppResult<()> {
        // `clock_timestamp()`, not `now()`, which is when the transaction began.
        let database = sqlx::query_scalar!(r#"SELECT clock_timestamp() AS "now!""#)
            .fetch_one(&self.pool)
            .await
            .map_err(AppError::internal)?;
        let worker = self.clock.now();
        let skew = worker.signed_duration_since(database).abs();
        if skew > CLOCK_REFUSED {
            return Err(AppError::internal(ClockSkew { worker, database }));
        }
        if skew > CLOCK_WARNED {
            tracing::warn!(
                worker = %worker,
                database = %database,
                skew_ms = skew.num_milliseconds(),
                "this worker's clock is drifting from the database's"
            );
        }
        Ok(())
    }

    /// Relays, backfills, schedules and runs until nothing is due by the
    /// worker's clock, including whatever the handlers publish. A failed
    /// delivery backs off past `now`, so this returns rather than retrying it.
    pub async fn run_until_idle(&self) -> AppResult<()> {
        loop {
            let relayed = self.relay().await?;
            let backfilled = self.backfill().await?;
            let scheduled = self.schedule().await?;
            let mut ran = false;
            while self.run_next().await? {
                ran = true;
            }
            if relayed == 0 && backfilled == 0 && scheduled == 0 && !ran {
                return Ok(());
            }
        }
    }

    /// Copies a batch of undispatched events into the queues registered for
    /// them in `messaging.subscription`, whatever this worker's own handlers.
    /// Returns how many events it dispatched; one nobody subscribes to is just
    /// marked dispatched. Only one relay runs at a time; another returns 0.
    ///
    /// A key's versions commit in order (each save waits on the row lock of the
    /// one before), so a statement that sees one sees every earlier one: with one
    /// relay at a time, reading in `seq` order, a key's earlier job always exists,
    /// ran, or was filtered out before its later one does.
    pub async fn relay(&self) -> AppResult<usize> {
        let mut tx = self.pool.begin().await.map_err(AppError::internal)?;
        // Its own statement, so the batch's snapshot is taken once the lock is held.
        let relaying = sqlx::query_scalar!(
            r#"SELECT pg_try_advisory_xact_lock($1) AS "locked!""#,
            RELAY_LOCK,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        if !relaying {
            return Ok(0);
        }
        let seqs = sqlx::query_scalar!(
            r#"
            SELECT seq FROM messaging.outbox
            WHERE dispatched_at IS NULL
            ORDER BY seq
            LIMIT $1
            "#,
            RELAY_BATCH,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        if seqs.is_empty() {
            return Ok(0);
        }
        // Begun only once there is a batch, so an idle worker's polls export nothing.
        let span = tracing::info_span!("relay_batch", events = seqs.len());
        self.dispatch(tx, &seqs).instrument(span).await?;
        Ok(seqs.len())
    }

    async fn dispatch(
        &self,
        mut tx: sqlx::Transaction<'static, sqlx::Postgres>,
        seqs: &[i64],
    ) -> AppResult<()> {
        let now = self.clock.now();
        sqlx::query!(
            r#"
            UPDATE messaging.outbox
            SET dispatched_at = $1, relay_batch = (SELECT nextval('messaging.relay_batch'))
            WHERE seq = ANY($2)
            "#,
            now,
            seqs,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        sqlx::query!(
            r#"
            INSERT INTO messaging.job
                (queue, event_type, key, version, payload, outbox_seq, run_at, max_attempts,
                 created_at)
            SELECT queue, event_type, key, version, payload, seq, $2, $3, $2
            FROM messaging.delivery
            WHERE seq = ANY($1)
            ORDER BY seq, queue
            "#,
            seqs,
            now,
            MAX_ATTEMPTS,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        tx.commit().await.map_err(AppError::internal)
    }

    /// Copies the next chunk (1000 events unless
    /// [`backfilling_in_chunks_of`](Self::backfilling_in_chunks_of) says
    /// otherwise) of one unfinished backfill into its queue, as jobs
    /// due now, and moves its cursor past them in the same transaction, so a
    /// worker that dies loses nothing and the next one carries on. Returns how
    /// many events it copied; 0 once no backfill has any left to copy. Any
    /// worker copies any backfill, as any relays any event; two never copy
    /// the same one at once, and neither waits for the other.
    ///
    /// The events are fixed when `migrate` registers the backfill, so the
    /// relay never races it, and compaction leaves them until they are
    /// copied. Copying in `seq` order copies each key's versions in order,
    /// and [`run_next`](Self::run_next) holds a key's live events back until
    /// its earlier ones are copied.
    pub async fn backfill(&self) -> AppResult<u64> {
        loop {
            match self.copy_chunk().await? {
                Some(0) => continue,
                Some(copied) => return Ok(copied),
                None => return Ok(0),
            }
        }
    }

    /// `None` when no backfill is left unfinished; `Some(0)` when one finished.
    async fn copy_chunk(&self) -> AppResult<Option<u64>> {
        let mut tx = self.pool.begin().await.map_err(AppError::internal)?;
        let Some(backfill) = sqlx::query!(
            r#"
            SELECT b.queue, b.event_type, b.topic, b.through_batch, b.after_seq,
                   s.retired_at IS NOT NULL AS "retired!"
            FROM messaging.backfill b
            JOIN messaging.subscription s USING (queue, event_type)
            WHERE b.completed_at IS NULL
            ORDER BY b.registered_at, b.queue, b.event_type
            LIMIT 1
            FOR UPDATE OF b SKIP LOCKED
            "#,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::internal)?
        else {
            return Ok(None);
        };
        let span = tracing::info_span!(
            "backfill_chunk",
            queue = backfill.queue,
            event_type = backfill.event_type,
            events = Empty,
        );
        async move {
            let now = self.clock.now();
            let chunk = if backfill.retired {
                None
            } else {
                let chunk = sqlx::query!(
                    r#"
                WITH chunk AS (
                    SELECT seq, key, version, payload FROM messaging.outbox
                    WHERE topic = $1 AND event_type = $2
                      AND seq > $3 AND relay_batch <= $4
                    ORDER BY seq
                    LIMIT $5
                ), copied AS (
                    INSERT INTO messaging.job
                        (queue, event_type, key, version, payload, outbox_seq, run_at,
                         max_attempts, created_at)
                    SELECT $6, $2, key, version, payload, seq, $7, $8, $7
                    FROM chunk ORDER BY seq
                )
                SELECT max(seq) AS last_seq, count(*) AS "copied!" FROM chunk
                "#,
                    backfill.topic,
                    backfill.event_type,
                    backfill.after_seq,
                    backfill.through_batch,
                    self.backfill_chunk,
                    backfill.queue,
                    now,
                    MAX_ATTEMPTS,
                )
                .fetch_one(&mut *tx)
                .await
                .map_err(AppError::internal)?;
                chunk.last_seq.map(|last_seq| (last_seq, chunk.copied))
            };
            // A retired subscription's backfill completes with nothing copied: its
            // queue's jobs are dead letters.
            let (after_seq, completed_at, copied) = match chunk {
                Some((last_seq, copied)) => (last_seq, None, copied),
                None => (backfill.after_seq, Some(now), 0),
            };
            sqlx::query!(
                r#"
            UPDATE messaging.backfill SET after_seq = $3, completed_at = $4
            WHERE queue = $1 AND event_type = $2
            "#,
                backfill.queue,
                backfill.event_type,
                after_seq,
                completed_at,
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
            if completed_at.is_some() {
                tracing::info!(
                    queue = backfill.queue,
                    event_type = backfill.event_type,
                    "backfill complete"
                );
            }
            tx.commit().await.map_err(AppError::internal)?;
            Span::current().record("events", copied);
            Ok(Some(copied as u64))
        }
        .instrument(span)
        .await
    }

    /// Turns each registered schedule whose next slot has passed by the
    /// worker's clock into one job, due now, for the last slot it missed, and
    /// moves its next slot past now: after downtime spanning several slots it
    /// runs once. A schedule with a run still pending or running (its job
    /// unfinished, or backing off) skips the slot, logged. Returns how many
    /// jobs it created. Any worker schedules any schedule, as any relays any
    /// event, by the expression `migrate` registered; two never create one
    /// slot twice, since each takes the schedule's row before moving it on.
    pub async fn schedule(&self) -> AppResult<usize> {
        let now = self.clock.now();
        let mut tx = self.pool.begin().await.map_err(AppError::internal)?;
        let due = sqlx::query!(
            r#"
            SELECT name, cron, next_at FROM messaging.schedule
            WHERE next_at <= $1
            ORDER BY next_at, name
            FOR UPDATE SKIP LOCKED
            "#,
            now,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        if due.is_empty() {
            return Ok(0);
        }
        let span = tracing::info_span!("schedule_slots", schedules = due.len(), jobs = Empty);
        async move {
            let mut created = 0;
            for schedule in due {
                let cron = match schedule.cron.parse::<Cron>() {
                    Ok(cron) => cron,
                    Err(err) => {
                        tracing::error!(schedule = schedule.name, error = %err, "schedule left unrun");
                        continue;
                    }
                };
                let Some(next_at) = cron.next_after(now) else {
                    tracing::error!(schedule = schedule.name, "schedule never falls due again");
                    continue;
                };
                let slot = cron.last_until(schedule.next_at, now);
                let payload = serde_json::to_value(Slot { slot }).map_err(AppError::internal)?;
                let inserted = sqlx::query!(
                    r#"
                    INSERT INTO messaging.job
                        (queue, event_type, key, version, payload, outbox_seq, run_at,
                         max_attempts, created_at, deferred)
                    VALUES ($1, $1, $1, 1, $2, 0, $3, $4, $3, true)
                    ON CONFLICT (queue, key) WHERE deferred AND dead_at IS NULL DO NOTHING
                    "#,
                    schedule.name,
                    payload,
                    now,
                    MAX_ATTEMPTS,
                )
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?
                .rows_affected()
                    == 1;
                if inserted {
                    created += 1;
                } else {
                    tracing::warn!(
                        schedule = schedule.name,
                        %slot,
                        "slot skipped: the schedule's previous run is still pending or running"
                    );
                }
                sqlx::query!(
                    "UPDATE messaging.schedule SET next_at = $2 WHERE name = $1",
                    schedule.name,
                    next_at,
                )
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            }
            tx.commit().await.map_err(AppError::internal)?;
            Span::current().record("jobs", created);
            Ok(created)
        }
        .instrument(span)
        .await
    }

    /// Deletes every relayed event older than its registered topic's
    /// retention by the worker's clock, but never its key's latest version,
    /// a deletion's included: the outbox keeps the latest state of every key.
    /// Nor one an unfinished backfill has yet to copy: its replacement may be
    /// later than the events [`caught_up`](crate::caught_up) waits for, so a
    /// copy would be served without its key until that one is handled.
    /// Returns how many it deleted. Runs alongside the relay, publishers and
    /// other compactions: a row is deleted only while a later version of its
    /// key exists, which nothing deletes unless a still later one does.
    /// Deliveries are copies, so `messaging.job` is untouched.
    #[tracing::instrument(name = "compaction", skip_all, fields(deleted = Empty))]
    pub async fn compact(&self) -> AppResult<u64> {
        let now = self.clock.now();
        let mut deleted = 0;
        loop {
            let batch = sqlx::query!(
                r#"
                DELETE FROM messaging.outbox WHERE seq IN (
                    SELECT o.seq FROM messaging.outbox o
                    JOIN messaging.topic t ON t.name = o.topic
                    WHERE o.dispatched_at IS NOT NULL
                      AND o.occurred_at < $1::timestamptz - t.retention
                      AND EXISTS (
                          SELECT 1 FROM messaging.outbox later
                          WHERE later.topic = o.topic AND later.key = o.key
                            AND later.version > o.version
                      )
                      AND NOT EXISTS (
                          SELECT 1 FROM messaging.backfill b
                          WHERE b.completed_at IS NULL AND b.topic = o.topic
                            AND b.event_type = o.event_type
                            AND o.seq > b.after_seq AND o.relay_batch <= b.through_batch
                      )
                    LIMIT $2
                )
                "#,
                now,
                COMPACT_BATCH,
            )
            .execute(&self.pool)
            .await
            .map_err(AppError::internal)?
            .rows_affected();
            deleted += batch;
            // Not `< COMPACT_BATCH`: a batch comes up short when another
            // compaction took some of its rows, with more still to delete.
            if batch == 0 {
                break;
            }
        }
        Span::current().record("deleted", deleted);
        tracing::info!(deleted, "outbox compacted");
        Ok(deleted)
    }

    /// [`compact`](Self::compact)s at a worker's first call, then at its
    /// first call once `compact_every` has passed since the last, by its
    /// clock. Workers don't coordinate: each compacts on its own schedule,
    /// which `compact` makes safe. The time is taken before compacting, so a
    /// failed compaction waits for the next period rather than the next poll;
    /// so does one refused for this worker's clock, when
    /// [`checking_its_clock`](Self::checking_its_clock).
    pub async fn compact_if_due(&self) -> AppResult<()> {
        let now = self.clock.now();
        {
            let mut last = self.last_compacted.lock().expect("not poisoned");
            if last.is_some_and(|last| now < last + self.compact_every) {
                return Ok(());
            }
            *last = Some(now);
        }
        if self.checks_clock_before_compacting {
            self.check_clock().await?;
        }
        self.compact().await?;
        Ok(())
    }

    /// Claims and runs the job due first, if any, of the subscriptions this
    /// worker serves, and of those only one whose key has no earlier version
    /// still to run in its queue: running, backing off, waiting for a worker
    /// that can decode it, or not yet copied by its queue's backfill. A job's
    /// row lasts until the transaction handling it commits, so a key's next
    /// version waits for that. The claim counts as an attempt, so a job whose
    /// worker died (or whose handler outran the lease) on its last attempt is
    /// dead-lettered when next claimed, without running its handler again.
    /// It looks up each subscription's first due job by index, so jobs no
    /// running worker serves cost nothing; it briefly locks one per
    /// subscription, which other workers skip.
    pub async fn run_next(&self) -> AppResult<bool> {
        let now = self.clock.now();
        let (queues, event_types): (Vec<_>, Vec<_>) = self
            .handlers
            .claims()
            .map(|c| (c.queue, c.event_type))
            .unzip();
        let claimed = sqlx::query_as!(
            Claimed,
            r#"
            UPDATE messaging.job SET attempts = attempts + 1, run_at = $2
            WHERE id = (
                SELECT due.id
                FROM unnest($3::text[], $4::text[]) AS served (queue, event_type)
                CROSS JOIN LATERAL (
                    SELECT id, run_at FROM messaging.job j
                    WHERE queue = served.queue AND event_type = served.event_type
                      AND dead_at IS NULL AND run_at <= $1
                      AND NOT EXISTS (
                          SELECT 1 FROM messaging.job earlier
                          WHERE earlier.queue = j.queue AND earlier.key = j.key
                            AND earlier.version < j.version AND earlier.dead_at IS NULL
                      )
                      AND NOT EXISTS (
                          SELECT 1 FROM messaging.backfill b
                          JOIN messaging.outbox uncopied
                            ON uncopied.topic = b.topic AND uncopied.event_type = b.event_type
                          WHERE b.queue = j.queue AND b.completed_at IS NULL
                            AND uncopied.key = j.key AND uncopied.version < j.version
                            AND uncopied.seq > b.after_seq
                            AND uncopied.relay_batch <= b.through_batch
                      )
                    ORDER BY run_at, id
                    LIMIT 1
                    FOR UPDATE SKIP LOCKED
                ) AS due
                ORDER BY due.run_at, due.id
                LIMIT 1
            )
            RETURNING id, queue, event_type, key, version, payload, attempts, max_attempts
            "#,
            now,
            now + LEASE,
            &queues as &[&str],
            &event_types as &[&str],
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;
        let Some(job) = claimed else {
            return Ok(false);
        };
        self.run_job(&job).instrument(job_span(&job)).await?;
        Ok(true)
    }

    async fn run_job(&self, job: &Claimed) -> AppResult<()> {
        if job.attempts > job.max_attempts {
            self.abandon(job).await?;
            Span::current().record("otel.status_code", "error");
        } else if let Err(err) = self.complete(job).await {
            self.fail(job, &err).await?;
            Span::current().record("otel.status_code", "error");
        }
        Ok(())
    }

    /// The handler's writes and the job's removal commit together, and only
    /// if this run removes the job, so a handler that only writes to the
    /// database takes effect exactly once, even past its lease: a run another
    /// worker completed first rolls its writes back. The removal ignores
    /// `dead_at`, so a dead letter whose slow handler succeeds is removed. A
    /// deferred job rescheduled or cancelled while it ran (a new `version`, or
    /// no row) is not removed, so that run's writes roll back too.
    async fn complete(&self, job: &Claimed) -> AppResult<()> {
        let mut tx = begin(&self.pool).await?;
        self.handlers
            .handle(&job.queue, &mut tx, job.payload.clone())
            .await?;
        let removed = sqlx::query!(
            "DELETE FROM messaging.job WHERE id = $1 AND version = $2",
            job.id,
            job.version,
        )
        .execute(conn(&mut tx))
        .await
        .map_err(AppError::internal)?
        .rows_affected();
        if removed == 0 {
            tracing::warn!(
                queue = job.queue,
                "job already completed by another worker (its lease ran out), or \
                 rescheduled or cancelled while it ran, so this run's writes are rolled back"
            );
            return Ok(());
        }
        tx.commit().await
    }

    /// A job dead-lettered while its handler ran (retired, or abandoned as
    /// unfinished) stays dead with the reason it was given, rather than
    /// being revived to retry; a deferred job rescheduled while it ran keeps
    /// its new time and attempts.
    async fn fail(&self, job: &Claimed, err: &AppError) -> AppResult<()> {
        let now = self.clock.now();
        let error = describe(err);
        let dead = job.attempts >= job.max_attempts;
        let recorded = sqlx::query!(
            r#"
            UPDATE messaging.job SET last_error = $2, run_at = $3, dead_at = $4
            WHERE id = $1 AND version = $5 AND dead_at IS NULL
            "#,
            job.id,
            error,
            now + backoff(job.attempts),
            dead.then_some(now),
            job.version,
        )
        .execute(&self.pool)
        .await
        .map_err(AppError::internal)?
        .rows_affected();
        if recorded == 0 {
            tracing::warn!(
                queue = job.queue,
                error,
                "job failed after it was dead-lettered, rescheduled or cancelled; left as it is"
            );
        } else if dead {
            tracing::error!(
                queue = job.queue,
                attempts = job.attempts,
                error,
                "job dead-lettered"
            );
        } else {
            tracing::warn!(
                queue = job.queue,
                attempts = job.attempts,
                error,
                "job failed; will retry"
            );
        }
        Ok(())
    }

    /// A slow handler's eventual `complete` still deletes the dead letter. A
    /// deferred job rescheduled since it was claimed is the new job, which
    /// starts its attempts afresh, so it is left to run.
    async fn abandon(&self, job: &Claimed) -> AppResult<()> {
        let abandoned = sqlx::query!(
            r#"
            UPDATE messaging.job
            SET dead_at = $2, attempts = max_attempts, last_error = concat_ws(': ', $3::text, last_error)
            WHERE id = $1 AND version = $4
            "#,
            job.id,
            self.clock.now(),
            UNFINISHED,
            job.version,
        )
        .execute(&self.pool)
        .await
        .map_err(AppError::internal)?
        .rows_affected();
        if abandoned == 0 {
            tracing::warn!(
                queue = job.queue,
                "unfinished job rescheduled or cancelled since it was claimed; left as it is"
            );
        } else {
            tracing::error!(
                queue = job.queue,
                attempts = job.max_attempts,
                error = UNFINISHED,
                "job dead-lettered"
            );
        }
        Ok(())
    }

    /// Every `E` published so far, oldest first: for tests and diagnostics.
    pub async fn published<E: Event>(&self) -> AppResult<Vec<EnvelopeOf<E>>> {
        self.published_after::<E>(0).await
    }

    /// Where the outbox ends now: pass it to
    /// [`published_after`](Self::published_after) to leave out what was
    /// published before, such as a test's seeds.
    pub async fn outbox_end(&self) -> AppResult<i64> {
        sqlx::query_scalar!(r#"SELECT coalesce(max(seq), 0) AS "end!" FROM messaging.outbox"#)
            .fetch_one(&self.pool)
            .await
            .map_err(AppError::internal)
    }

    /// Every `E` published since the outbox ended at `end`, oldest first.
    pub async fn published_after<E: Event>(&self, end: i64) -> AppResult<Vec<EnvelopeOf<E>>> {
        let payloads = sqlx::query_scalar!(
            r#"
            SELECT payload FROM messaging.outbox
            WHERE topic = $1 AND event_type = $2 AND seq > $3
            ORDER BY seq
            "#,
            <E::Topic as Topic>::NAME,
            E::TYPE,
            end,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::internal)?;
        payloads
            .into_iter()
            .map(|payload| serde_json::from_value(payload).map_err(AppError::internal))
            .collect()
    }

    pub async fn dead_letters(&self) -> AppResult<Vec<DeadLetter>> {
        sqlx::query_as!(
            DeadLetter,
            r#"
            SELECT queue, payload, attempts, last_error FROM messaging.job
            WHERE dead_at IS NOT NULL
            ORDER BY dead_at, id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::internal)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("`{queue}` isn't subscribed to `{event_type}`: run `migrate` with this build first")]
struct Unregistered {
    queue: &'static str,
    event_type: &'static str,
}

#[derive(Debug, thiserror::Error)]
#[error("the schedule `{0}` isn't registered: run `migrate` with this build first")]
struct UnregisteredSchedule(&'static str);

#[derive(Debug, thiserror::Error)]
#[error("`{queue}` is registered to the topic `{registered}`, but this build serves `{served}`")]
struct OtherTopic {
    queue: &'static str,
    registered: String,
    served: &'static str,
}

#[derive(Debug, thiserror::Error)]
#[error(
    "this worker's clock reads {worker}, {} the database's {database}: fix its time sync",
    apart(*worker - *database)
)]
struct ClockSkew {
    worker: DateTime<Utc>,
    database: DateTime<Utc>,
}

fn apart(skew: TimeDelta) -> String {
    let side = if skew > TimeDelta::zero() {
        "ahead of"
    } else {
        "behind"
    };
    format!("{}s {side}", skew.num_seconds().abs())
}

/// One run of `job`: a child of the span that published its event, so each
/// retry is a sibling in that trace; the root of a new trace when its envelope
/// names none. Its own root either way, never a child of the worker's poll.
fn job_span(job: &Claimed) -> Span {
    let span = tracing::info_span!(
        parent: None,
        "job",
        otel.kind = "consumer",
        otel.status_code = Empty,
        messaging.destination.name = job.queue,
        event_type = job.event_type,
        key = job.key,
        version = job.version,
        attempt = job.attempts,
    );
    crate::trace::continue_published(&span, &job.payload);
    span
}

/// 10s, doubling per attempt, at most an hour.
fn backoff(attempts: i32) -> TimeDelta {
    let exponent = attempts.saturating_sub(1).clamp(0, 9) as u32;
    TimeDelta::seconds(10 * 2_i64.pow(exponent)).min(TimeDelta::hours(1))
}

/// `AppError::Internal` displays only "internal error"; the job keeps its cause.
fn describe(err: &AppError) -> String {
    match err {
        AppError::Internal(source) => source.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_back_off_exponentially_up_to_an_hour() {
        assert_eq!(backoff(1), TimeDelta::seconds(10));
        assert_eq!(backoff(2), TimeDelta::seconds(20));
        assert_eq!(backoff(5), TimeDelta::seconds(160));
        assert_eq!(backoff(50), TimeDelta::hours(1));
    }
}
