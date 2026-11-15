//! A read module's own copy of another module's roots, kept from their topic
//! (ADR 0036): a projection. The framework decides whether an event is newer
//! than what the copy holds and keeps a tombstone for a removed key; the module
//! only writes its rows. [`Generations`] says which copy to serve from: the
//! newest that has [`caught_up`], having handled every event its topics held
//! when `migrate` registered it.

use crate::unit_of_work::conn;
use app_error::{AppError, AppResult};
use app_events::{Subscribable, Topic, TopicEnvelope};
use app_persistence::{Handle, Handlers, Tx};
use async_trait::async_trait;
use sqlx::PgPool;
use std::marker::PhantomData;
use std::sync::Mutex;

/// A projection's rows for the roots of `T`, one per key. Each method runs in
/// the delivery's transaction and only once the event is known to be newer
/// than the row, so it writes unguarded: `put` replaces the key's row (an
/// `INSERT … ON CONFLICT (id) DO UPDATE`), `remove` deletes it.
#[async_trait]
pub trait Project<T: Topic>: Send + Sync {
    /// Whether `event` ends its key, so its row is removed: the topic's deletion.
    fn removes(&self, event: &T::Event) -> bool;

    /// Writes the key's row as the event's entity now is.
    async fn put(&self, tx: &mut Tx, event: &TopicEnvelope<T>) -> AppResult<()>;

    async fn remove(&self, tx: &mut Tx, key: &str) -> AppResult<()>;
}

/// Subscribes a projection to the whole of its topic from `earliest`.
pub trait Projecting {
    /// `queue` names the projection as well as its subscription; rebuilding the
    /// copy in a new shape is a new queue (`<module>.<copy>_v2`) beside the old.
    fn project<T>(self, queue: &'static str, rows: impl Project<T> + 'static) -> Self
    where
        T: Topic + Subscribable<Envelope = TopicEnvelope<T>>;
}

impl Projecting for Handlers {
    fn project<T>(self, queue: &'static str, rows: impl Project<T> + 'static) -> Self
    where
        T: Topic + Subscribable<Envelope = TopicEnvelope<T>>,
    {
        self.on_from_earliest::<T>(
            queue,
            Projection {
                queue,
                rows,
                topic: PhantomData,
            },
        )
    }
}

struct Projection<T, P> {
    queue: &'static str,
    rows: P,
    topic: PhantomData<fn() -> T>,
}

#[async_trait]
impl<T, P> Handle<T> for Projection<T, P>
where
    T: Topic + Subscribable<Envelope = TopicEnvelope<T>>,
    P: Project<T>,
{
    async fn handle(&self, tx: &mut Tx, event: TopicEnvelope<T>) -> AppResult<()> {
        let removes = self.rows.removes(&event.event);
        let newer = sqlx::query_scalar!(
            r#"
            INSERT INTO messaging.projection_version (queue, key, version, removed)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (queue, key) DO UPDATE
            SET version = EXCLUDED.version, removed = EXCLUDED.removed
            WHERE projection_version.version < EXCLUDED.version
            RETURNING true AS "newer!"
            "#,
            self.queue,
            event.key,
            event.version,
            removes,
        )
        .fetch_optional(conn(tx))
        .await
        .map_err(AppError::internal)?
        .is_some();
        match (newer, removes) {
            (false, _) => Ok(()),
            (true, true) => self.rows.remove(tx, &event.key).await,
            (true, false) => self.rows.put(tx, &event).await,
        }
    }
}

/// Whether every one of `queues` is registered and has caught up with its
/// backfill: every event its topics held when `migrate` registered it, relayed
/// by then or not, is copied or relayed into the queue and handled (or dead).
/// Those are the events at or below the outbox's highest `seq` at registration;
/// a later one is live and holds nothing back. A copy fed by them then holds
/// every key its topics held at registration, and keeps up from there, except
/// those of its dead letters: they don't hold it back, since one poisoned event
/// would take the whole read model down, and [`Generations`] logs them at error
/// when it first serves the copy.
pub async fn caught_up(pool: &PgPool, queues: &[&str]) -> AppResult<bool> {
    let mut queues: Vec<&str> = queues.to_vec();
    queues.sort_unstable();
    queues.dedup();
    sqlx::query_scalar!(
        r#"
        SELECT
            (SELECT count(DISTINCT queue) FROM messaging.subscription
             WHERE queue = ANY($1) AND retired_at IS NULL) = cardinality($1)
            AND NOT EXISTS (
                SELECT 1 FROM messaging.backfill b
                WHERE b.queue = ANY($1)
                  AND (b.completed_at IS NULL OR EXISTS (
                      SELECT 1 FROM messaging.outbox o
                      WHERE o.dispatched_at IS NULL AND o.topic = b.topic
                        AND o.event_type = b.event_type AND o.seq <= b.through_seq
                  ) OR EXISTS (
                      SELECT 1 FROM messaging.job j
                      WHERE j.queue = b.queue AND j.event_type = b.event_type
                        AND j.dead_at IS NULL AND j.outbox_seq <= b.through_seq
                  ))
            ) AS "caught_up!"
        "#,
        &queues as &[&str],
    )
    .fetch_one(pool)
    .await
    .map_err(AppError::internal)
}

/// How many dead letters each of `queues` holds, for those holding any.
pub async fn dead_letters_in(pool: &PgPool, queues: &[&str]) -> AppResult<Vec<(String, i64)>> {
    sqlx::query!(
        r#"
        SELECT queue, count(*) AS "dead_letters!" FROM messaging.job
        WHERE queue = ANY($1) AND dead_at IS NOT NULL
        GROUP BY queue
        "#,
        queues as &[&str],
    )
    .fetch_all(pool)
    .await
    .map(|counts| {
        counts
            .into_iter()
            .map(|c| (c.queue, c.dead_letters))
            .collect()
    })
    .map_err(AppError::internal)
}

/// Which generation of a read module's projections to serve from: the newest
/// whose queues have all [`caught_up`]. A rebuilt copy (new tables fed by
/// `_v2` queues) is listed before the one it replaces and is switched to once
/// it catches up, with no deploy; the old one is retired in a later change.
/// Once a generation is served it stays served, so only newer ones are checked
/// again, and none once the newest is.
pub struct Generations<G> {
    pool: PgPool,
    newest_first: Vec<(G, &'static [&'static str])>,
    serving: Mutex<Option<usize>>,
}

impl<G: Copy> Generations<G> {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            newest_first: Vec::new(),
            serving: Mutex::new(None),
        }
    }

    /// The next older generation, fed by `queues`.
    pub fn then(mut self, generation: G, queues: &'static [&'static str]) -> Self {
        self.newest_first.push((generation, queues));
        self
    }

    /// The newest generation that has caught up; `unavailable` while none has,
    /// such as before the first one's backfill is handled.
    pub async fn serving(&self) -> AppResult<G> {
        let known = *self.serving.lock().expect("not poisoned");
        let newer = known.unwrap_or(self.newest_first.len());
        for (at, (_, queues)) in self.newest_first[..newer].iter().enumerate() {
            if caught_up(&self.pool, queues).await? {
                let (at, moved) = {
                    let mut serving = self.serving.lock().expect("not poisoned");
                    let before = *serving;
                    let at = newest(&mut serving, at);
                    (at, before != Some(at))
                };
                let (generation, queues) = self.newest_first[at];
                if moved {
                    self.report_dead_letters(queues).await;
                }
                return Ok(generation);
            }
        }
        match *self.serving.lock().expect("not poisoned") {
            Some(at) => Ok(self.newest_first[at].0),
            None => Err(AppError::unavailable(
                "this read model is still being built: retry later",
            )),
        }
    }
}

impl<G> Generations<G> {
    /// Dead letters don't hold a copy back from being served (ADR 0036), so a
    /// copy missing their keys is reported here instead, once per generation
    /// per process.
    async fn report_dead_letters(&self, queues: &[&str]) {
        match dead_letters_in(&self.pool, queues).await {
            Ok(counts) => {
                for (queue, dead_letters) in counts {
                    tracing::error!(
                        queue,
                        dead_letters,
                        "serving a projection with dead-lettered deliveries: its copy is missing or stale for their keys until they are rerun"
                    );
                }
            }
            Err(error) => tracing::warn!(
                %error,
                "couldn't count the dead letters of a projection now served"
            ),
        }
    }
}

/// Caches `at` unless a newer generation (a smaller index) is cached already,
/// which a concurrent `serving` may have done while this one awaited a check.
fn newest(serving: &mut Option<usize>, at: usize) -> usize {
    let newest = serving.map_or(at, |cached| cached.min(at));
    *serving = Some(newest);
    newest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_generation_caught_up_after_a_newer_one_is_served_leaves_the_newer_one_served() {
        let mut serving = Some(0);
        assert_eq!(newest(&mut serving, 1), 0);
        assert_eq!(serving, Some(0));
    }
}
