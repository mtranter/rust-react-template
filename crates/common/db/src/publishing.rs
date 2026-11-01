use crate::{conn, map_db_error};
use app_domain::Clock;
use app_error::{AppError, AppResult};
use app_persistence::{EventPublisher, PendingEvent, Tx};
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

/// Publishes to `messaging.outbox`, in the caller's transaction, for the worker
/// to relay once it commits. The clock stamps each envelope.
pub struct PgEventPublisher {
    clock: Arc<dyn Clock>,
}

impl PgEventPublisher {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self { clock }
    }
}

#[async_trait]
impl EventPublisher for PgEventPublisher {
    async fn publish(&self, tx: &mut Tx, event: PendingEvent) -> AppResult<()> {
        let now = self.clock.now();
        let envelope = event.envelope(Uuid::new_v4(), now)?;
        sqlx::query!(
            r#"
            INSERT INTO messaging.outbox (topic, key, version, event_type, payload, occurred_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            envelope.topic,
            envelope.partition_key,
            envelope.entity_version,
            envelope.event_type,
            serde_json::to_value(&envelope).map_err(AppError::internal)?,
            now,
        )
        .execute(conn(tx))
        .await
        .map_err(|err| {
            map_db_error(err, |constraint| {
                (constraint == "outbox_event_idx").then(|| {
                    AppError::internal(SecondEvent {
                        topic: envelope.topic.clone(),
                        key: envelope.partition_key.clone(),
                        version: envelope.entity_version,
                    })
                })
            })
        })?;
        Ok(())
    }
}

/// Two events announcing one version of a root: a save publishes exactly one.
#[derive(Debug, thiserror::Error)]
#[error("`{key}` of `{topic}` already announced its version {version}")]
struct SecondEvent {
    topic: String,
    key: String,
    version: i64,
}
