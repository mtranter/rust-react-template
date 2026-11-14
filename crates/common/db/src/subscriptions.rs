//! Subscriptions (ADR 0014): which queues the relay fans each event out to,
//! kept in `messaging.subscription` rather than in each process's memory, so
//! every build running during a rolling deploy relays the same way.

use crate::worker::RELAY_LOCK;
use app_domain::Clock;
use app_persistence::{Handlers, MAX_ATTEMPTS, Start, Subscription};
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
/// it commits, and one starting from [`Start::Earliest`] is first backfilled
/// with every relayed event of its type the outbox holds, due by `clock`.
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

/// Under the relay's lock, so each event reaches a new queue once: every
/// relayed one now, every other by the relay once this commits. A key's relayed
/// events are a prefix of its versions, so its live ones still come after.
/// Every subscription in one statement, so a compaction can't run between two
/// event types and leave a gap in a key's prefix: one snapshot keeps it whole.
async fn backfill(
    conn: &mut PgConnection,
    clock: &dyn Clock,
    subscriptions: &[Subscription],
) -> sqlx::Result<()> {
    // Its own statement, so the backfill's snapshot is taken once the lock is held.
    sqlx::query!("SELECT pg_advisory_xact_lock($1)", RELAY_LOCK)
        .execute(&mut *conn)
        .await?;
    let queues: Vec<&str> = subscriptions.iter().map(|s| s.queue).collect();
    let event_types: Vec<&str> = subscriptions.iter().map(|s| s.event_type).collect();
    sqlx::query!(
        r#"
        INSERT INTO messaging.job
            (queue, event_type, key, version, payload, run_at, max_attempts, created_at)
        SELECT queue, event_type, key, version, payload, $1, $2, $1
        FROM messaging.delivery
        WHERE dispatched_at IS NOT NULL
          AND (queue, event_type) IN (SELECT * FROM unnest($3::text[], $4::text[]))
        ORDER BY seq, queue
        "#,
        clock.now(),
        MAX_ATTEMPTS,
        &queues as &[&str],
        &event_types as &[&str],
    )
    .execute(conn)
    .await?;
    Ok(())
}
