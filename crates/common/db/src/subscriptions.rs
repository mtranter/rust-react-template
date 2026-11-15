//! Subscriptions (ADR 0014): which queues the relay fans each event out to,
//! kept in `messaging.subscription` rather than in each process's memory, so
//! every build running during a rolling deploy relays the same way.

use crate::worker::RELAY_LOCK;
use app_domain::Clock;
use app_persistence::{Handlers, Start, Subscription};
use sqlx::{PgConnection, PgPool};

/// Why `migrate` refused to register a build's subscriptions.
#[derive(Debug, thiserror::Error)]
pub enum SubscriptionError {
    #[error(
        "the queue `{queue}` is bound to the topic `{registered}`, not `{requested}`: subscribe a new queue to a new topic"
    )]
    TopicChanged {
        queue: String,
        registered: String,
        requested: &'static str,
    },
    #[error("the queue `{0}` was retired, and a retired queue's name is never reused")]
    Retired(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Registers every subscription `handlers` serve: `migrate` runs it after the
/// migrations, before a rollout, so a build's subscriptions exist before any
/// of its processes publishes or relays. Each receives the events relayed after
/// it commits, and one starting from [`Start::Earliest`] is also backfilled,
/// by the worker, with every relayed event of its type the outbox holds.
/// Registering again changes nothing, a changed start included.
pub async fn register_subscriptions(
    pool: &PgPool,
    clock: &dyn Clock,
    handlers: &Handlers,
) -> Result<(), SubscriptionError> {
    let mut tx = pool.begin().await?;
    let mut from_earliest = Vec::new();
    // Two `migrate`s at once must not both find a queue unregistered.
    sqlx::query!("LOCK TABLE messaging.subscription IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    for subscription in handlers.subscriptions() {
        let Subscription {
            queue,
            topic,
            event_type,
            starts,
        } = subscription;
        let registered = sqlx::query!(
            r#"
            SELECT event_type, topic, starts, retired_at IS NOT NULL AS "retired!"
            FROM messaging.subscription WHERE queue = $1
            "#,
            queue,
        )
        .fetch_all(&mut *tx)
        .await?;
        if let Some(other) = registered.iter().find(|r| r.topic != topic) {
            return Err(SubscriptionError::TopicChanged {
                queue: queue.into(),
                registered: other.topic.clone(),
                requested: topic,
            });
        }
        if registered.iter().any(|r| r.retired) {
            return Err(SubscriptionError::Retired(queue.into()));
        }
        if let Some(other) = registered
            .iter()
            .find(|r| r.event_type == event_type && r.starts != starts.as_str())
        {
            tracing::warn!(
                queue,
                event_type,
                registered = other.starts,
                declared = starts.as_str(),
                "a registered subscription's start never changes"
            );
        }
        let inserted = sqlx::query!(
            r#"
            INSERT INTO messaging.subscription (queue, event_type, topic, starts, registered_at)
            VALUES ($1, $2, $3, $4, now())
            ON CONFLICT DO NOTHING
            "#,
            queue,
            event_type,
            topic,
            starts.as_str(),
        )
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        if inserted && starts == Start::Earliest {
            from_earliest.push(subscription);
        }
    }
    if !from_earliest.is_empty() {
        backfill(&mut tx, clock, &from_earliest).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Under the relay's lock, so each event reaches a new queue once: those of
/// every batch relayed so far by the backfill, every later one by the relay. The
/// copying is the worker's ([`Worker::backfill`](crate::Worker::backfill)), so
/// this stays one short transaction however long the topic's outbox.
async fn backfill(
    conn: &mut PgConnection,
    clock: &dyn Clock,
    subscriptions: &[Subscription],
) -> sqlx::Result<()> {
    sqlx::query!("SELECT pg_advisory_xact_lock($1)", RELAY_LOCK)
        .execute(&mut *conn)
        .await?;
    let queues: Vec<&str> = subscriptions.iter().map(|s| s.queue).collect();
    let event_types: Vec<&str> = subscriptions.iter().map(|s| s.event_type).collect();
    let topics: Vec<&str> = subscriptions.iter().map(|s| s.topic).collect();
    // A sequence isn't transactional: read under the lock, its value is the last
    // committed batch's, or a rolled-back one's later still, which owns no events.
    // A backfill with nothing to copy completes here, so a queue gaining an event
    // type new to its topic stays caught up.
    let completed = sqlx::query!(
        r#"
        WITH registering AS (
            SELECT (SELECT CASE WHEN is_called THEN last_value ELSE 0 END
                    FROM messaging.relay_batch) AS through_batch,
                   (SELECT coalesce(max(seq), 0) FROM messaging.outbox) AS through_seq
        )
        INSERT INTO messaging.backfill
            (queue, event_type, topic, through_batch, through_seq, registered_at, completed_at)
        SELECT s.queue, s.event_type, s.topic, r.through_batch, r.through_seq, $1::timestamptz,
               CASE WHEN EXISTS (
                   SELECT 1 FROM messaging.outbox o
                   WHERE o.topic = s.topic AND o.event_type = s.event_type
                     AND o.relay_batch <= r.through_batch
               ) THEN NULL ELSE $1::timestamptz END
        FROM unnest($2::text[], $3::text[], $4::text[]) AS s (queue, event_type, topic)
        CROSS JOIN registering r
        RETURNING queue, event_type, completed_at IS NOT NULL AS "completed!"
        "#,
        clock.now(),
        &queues as &[&str],
        &event_types as &[&str],
        &topics as &[&str],
    )
    .fetch_all(conn)
    .await?;
    for backfill in completed.iter().filter(|b| b.completed) {
        tracing::info!(
            queue = backfill.queue,
            event_type = backfill.event_type,
            "backfill complete"
        );
    }
    Ok(())
}
