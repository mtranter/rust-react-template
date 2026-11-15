use app_db::{Project, conn};
use app_error::{AppError, AppResult};
use app_events::TopicEnvelope;
use app_persistence::Tx;
use async_trait::async_trait;
use widgets_api::events::v1::{WidgetEvent, WidgetTopic};

/// The queue feeding the first generation of the copy: renaming it strands
/// the copy, so a new shape is a new generation (a `_v2` queue and table).
pub const COPY: &str = "widget_index.copy";

pub struct EntryRows;

#[async_trait]
impl Project<WidgetTopic> for EntryRows {
    fn removes(&self, _event: &WidgetEvent) -> bool {
        false
    }

    async fn put(&self, tx: &mut Tx, event: &TopicEnvelope<WidgetTopic>) -> AppResult<()> {
        let entity = serde_json::to_value(&event.entity).map_err(AppError::internal)?;
        sqlx::query!(
            r#"
            INSERT INTO widget_index.entry (key, entity, updated_at) VALUES ($1, $2, $3)
            ON CONFLICT (key) DO UPDATE
            SET entity = EXCLUDED.entity, updated_at = EXCLUDED.updated_at
            "#,
            event.key,
            entity,
            event.time,
        )
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }

    async fn remove(&self, tx: &mut Tx, key: &str) -> AppResult<()> {
        sqlx::query!("DELETE FROM widget_index.entry WHERE key = $1", key)
            .execute(conn(tx))
            .await
            .map_err(AppError::internal)?;
        Ok(())
    }
}
