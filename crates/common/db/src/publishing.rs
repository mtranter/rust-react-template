use crate::conn;
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
        let event_type = event.attributes().event_type;
        let envelope = event.envelope(Uuid::new_v4(), now)?;
        sqlx::query!(
            r#"
            INSERT INTO messaging.outbox (event_type, payload, occurred_at)
            VALUES ($1, $2, $3)
            "#,
            event_type,
            serde_json::to_value(&envelope).map_err(AppError::internal)?,
            now,
        )
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }
}
