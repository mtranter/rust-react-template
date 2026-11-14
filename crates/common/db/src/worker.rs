//! Delivers what units of work leave in `messaging` (ADR 0013): the relay copies
//! each outbox event into one delivery per subscriber, a row of `messaging.job`,
//! then each runs in its own transaction, with retries and dead-lettering, in
//! order per key within each queue. Any number of workers can run against one
//! database: one relays at a time, every claim skips rows another worker holds,
//! and a worker claims only the subscriptions its own handlers can decode, so
//! builds can differ during a rolling deploy.

use crate::unit_of_work::{begin, conn};
use app_domain::Clock;
use app_error::{AppError, AppResult};
use app_events::{EnvelopeOf, Event, Topic};
use app_persistence::{Handlers, MAX_ATTEMPTS};
use chrono::{DateTime, TimeDelta, Utc};
use serde_json::Value;
use sqlx::PgPool;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long a claimed job is hidden from other workers. A handler that runs
/// longer may run twice at once; one whose worker dies runs again after it.
const LEASE: TimeDelta = TimeDelta::minutes(5);
const RELAY_BATCH: i64 = 100;
/// Each its own transaction, so a compaction never holds the outbox for long.
const COMPACT_BATCH: i64 = 1000;
/// The advisory lock only one relay holds at a time: `"relay"` in ASCII.
pub(crate) const RELAY_LOCK: i64 = 0x72_656c_6179;

pub struct Worker {
    pool: PgPool,
    clock: Arc<dyn Clock>,
    handlers: Arc<Handlers>,
    compact_every: TimeDelta,
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
            last_compacted: Mutex::new(None),
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

    /// Polls every `interval` until `shutdown` resolves, compacting the
    /// outbox after each poll that it's due. Refuses to start unless
    /// [`check_subscriptions`](Self::check_subscriptions) passes; after that,
    /// a failure to reach the database is logged and retried at the next poll.
    pub async fn run(
        &self,
        interval: Duration,
        shutdown: impl Future<Output = ()>,
    ) -> AppResult<()> {
        self.check_subscriptions().await?;
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

    /// Relays and runs until nothing is due by the worker's clock, including
    /// whatever the handlers publish. A failed delivery backs off past
    /// `now`, so this returns rather than retrying it.
    pub async fn run_until_idle(&self) -> AppResult<()> {
        loop {
            let relayed = self.relay().await?;
            let mut ran = false;
            while self.run_next().await? {
                ran = true;
            }
            if relayed == 0 && !ran {
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

        let now = self.clock.now();
        sqlx::query!(
            "UPDATE messaging.outbox SET dispatched_at = $1 WHERE seq = ANY($2)",
            now,
            &seqs,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        sqlx::query!(
            r#"
            INSERT INTO messaging.job
                (queue, event_type, key, version, payload, run_at, max_attempts, created_at)
            SELECT queue, event_type, key, version, payload, $2, $3, $2
            FROM messaging.delivery
            WHERE seq = ANY($1)
            ORDER BY seq, queue
            "#,
            &seqs,
            now,
            MAX_ATTEMPTS,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        tx.commit().await.map_err(AppError::internal)?;
        Ok(seqs.len())
    }

    /// Deletes every relayed event older than its registered topic's
    /// retention by the worker's clock, but never its key's latest version,
    /// a deletion's included: the outbox keeps the latest state of every key.
    /// Returns how many it deleted. Runs alongside the relay, publishers and
    /// other compactions: a row is deleted only while a later version of its
    /// key exists, which nothing deletes unless a still later one does.
    /// Deliveries are copies, so `messaging.job` is untouched.
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
        tracing::info!(deleted, "outbox compacted");
        Ok(deleted)
    }

    /// [`compact`](Self::compact)s at a worker's first call, then at its
    /// first call once `compact_every` has passed since the last, by its
    /// clock. Workers don't coordinate: each compacts on its own schedule,
    /// which `compact` makes safe. The time is taken before compacting, so a
    /// failed compaction waits for the next period rather than the next poll.
    pub async fn compact_if_due(&self) -> AppResult<()> {
        let now = self.clock.now();
        {
            let mut last = self.last_compacted.lock().expect("not poisoned");
            if last.is_some_and(|last| now < last + self.compact_every) {
                return Ok(());
            }
            *last = Some(now);
        }
        self.compact().await?;
        Ok(())
    }

    /// Claims and runs the job due first, if any, of the subscriptions this
    /// worker serves, and of those only one whose key has no earlier version
    /// still to run in its queue: running, backing off, or waiting for a worker
    /// that can decode it. A job's row lasts until the transaction handling it
    /// commits, so a key's next version waits for that. The claim counts as an
    /// attempt, so a job that kills its worker is dead-lettered too, eventually.
    /// It looks up each subscription's first due job by index, so jobs no
    /// running worker serves cost nothing; it briefly locks one per
    /// subscription, which other workers skip.
    pub async fn run_next(&self) -> AppResult<bool> {
        let now = self.clock.now();
        let (queues, event_types): (Vec<_>, Vec<_>) = self
            .handlers
            .subscriptions()
            .map(|s| (s.queue, s.event_type))
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
                    ORDER BY run_at, id
                    LIMIT 1
                    FOR UPDATE SKIP LOCKED
                ) AS due
                ORDER BY due.run_at, due.id
                LIMIT 1
            )
            RETURNING id, queue, payload, attempts, max_attempts
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

        if let Err(err) = self.complete(&job).await {
            self.fail(&job, &err).await?;
        }
        Ok(true)
    }

    /// The handler's writes and the job's removal commit together, so a
    /// handler that only writes to the database takes effect exactly once.
    async fn complete(&self, job: &Claimed) -> AppResult<()> {
        let mut tx = begin(&self.pool).await?;
        self.handlers
            .handle(&job.queue, &mut tx, job.payload.clone())
            .await?;
        sqlx::query!("DELETE FROM messaging.job WHERE id = $1", job.id)
            .execute(conn(&mut tx))
            .await
            .map_err(AppError::internal)?;
        tx.commit().await
    }

    async fn fail(&self, job: &Claimed, err: &AppError) -> AppResult<()> {
        let now = self.clock.now();
        let error = describe(err);
        let dead = job.attempts >= job.max_attempts;
        if dead {
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
        sqlx::query!(
            r#"
            UPDATE messaging.job SET last_error = $2, run_at = $3, dead_at = $4
            WHERE id = $1
            "#,
            job.id,
            error,
            now + backoff(job.attempts),
            dead.then_some(now),
        )
        .execute(&self.pool)
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }

    /// Every `E` published so far, oldest first: for tests and diagnostics.
    pub async fn published<E: Event>(&self) -> AppResult<Vec<EnvelopeOf<E>>> {
        let payloads = sqlx::query_scalar!(
            r#"
            SELECT payload FROM messaging.outbox
            WHERE topic = $1 AND event_type = $2
            ORDER BY seq
            "#,
            <E::Topic as Topic>::NAME,
            E::TYPE,
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
#[error("`{queue}` is registered to the topic `{registered}`, but this build serves `{served}`")]
struct OtherTopic {
    queue: &'static str,
    registered: String,
    served: &'static str,
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
