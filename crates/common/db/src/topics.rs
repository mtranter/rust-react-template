//! Topics: how long each keeps its history, kept in `messaging.topic` rather
//! than in each process's memory, so every build running during a rolling
//! deploy compacts the outbox alike, including topics its worker doesn't
//! subscribe to.

use app_error::{AppError, AppResult};
use app_events::PublishedTopic;
use sqlx::PgPool;
use sqlx::postgres::types::PgInterval;

/// Registers every topic a build publishes: `migrate` runs it after the
/// migrations. Registering again takes a changed retention.
pub async fn register_topics(pool: &PgPool, topics: &[PublishedTopic]) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(AppError::internal)?;
    for topic in topics {
        let retention = PgInterval::try_from(topic.retention)
            .map_err(|err| AppError::internal(sqlx::Error::Encode(err)))?;
        sqlx::query!(
            r#"
            INSERT INTO messaging.topic (name, retention, registered_at)
            VALUES ($1, $2, now())
            ON CONFLICT (name) DO UPDATE SET retention = EXCLUDED.retention
            "#,
            topic.name,
            retention,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    }
    tx.commit().await.map_err(AppError::internal)
}
