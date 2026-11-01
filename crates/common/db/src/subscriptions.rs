//! Subscriptions (spec §4): which queues the relay fans each event out to,
//! kept in `messaging.subscription` rather than in each process's memory, so
//! every build running during a rolling deploy relays the same way.

use app_persistence::{Handlers, Subscription};
use sqlx::PgPool;

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
/// it commits, none before. Registering again changes nothing.
pub async fn register_subscriptions(
    pool: &PgPool,
    handlers: &Handlers,
) -> Result<(), SubscriptionError> {
    let mut tx = pool.begin().await?;
    // Two `migrate`s at once must not both find a queue unregistered.
    sqlx::query!("LOCK TABLE messaging.subscription IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    for Subscription {
        queue,
        topic,
        event_type,
    } in handlers.subscriptions()
    {
        let registered = sqlx::query!(
            r#"
            SELECT topic, retired_at IS NOT NULL AS "retired!"
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
        sqlx::query!(
            r#"
            INSERT INTO messaging.subscription (queue, event_type, topic, registered_at)
            VALUES ($1, $2, $3, now())
            ON CONFLICT DO NOTHING
            "#,
            queue,
            event_type,
            topic,
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
