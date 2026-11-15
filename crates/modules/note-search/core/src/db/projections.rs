use app_db::{Project, conn};
use app_error::{AppError, AppResult};
use app_events::TopicEnvelope;
use app_persistence::Tx;
use async_trait::async_trait;
use notes_api::events::v1::{NoteEvent, NoteTopic};
use users_api::events::v1::{UserEvent, UserTopic};
use uuid::Uuid;

/// The queues feeding the first generation of tables: renaming one strands
/// its copy, so a new shape is a new generation (`_v2` queues and tables).
pub const COPY_NOTES: &str = "note_search.copy_notes";
pub const COPY_AUTHORS: &str = "note_search.copy_authors";

fn id(key: &str) -> AppResult<Uuid> {
    Uuid::parse_str(key).map_err(AppError::internal)
}

pub struct NoteRows;

#[async_trait]
impl Project<NoteTopic> for NoteRows {
    fn removes(&self, event: &NoteEvent) -> bool {
        matches!(event, NoteEvent::Deleted)
    }

    async fn put(&self, tx: &mut Tx, event: &TopicEnvelope<NoteTopic>) -> AppResult<()> {
        let note = &event.entity;
        // An event published before notes carried `createdAt` can only say
        // when it was itself recorded.
        let created_at = note.created_at.unwrap_or(event.time);
        sqlx::query!(
            r#"
            INSERT INTO note_search.note (id, author_id, title, created_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (id) DO UPDATE
            SET author_id = EXCLUDED.author_id, title = EXCLUDED.title,
                created_at = EXCLUDED.created_at
            "#,
            note.id,
            note.author_id,
            note.title,
            created_at,
        )
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }

    async fn remove(&self, tx: &mut Tx, key: &str) -> AppResult<()> {
        sqlx::query!("DELETE FROM note_search.note WHERE id = $1", id(key)?)
            .execute(conn(tx))
            .await
            .map_err(AppError::internal)?;
        Ok(())
    }
}

pub struct AuthorRows;

#[async_trait]
impl Project<UserTopic> for AuthorRows {
    fn removes(&self, event: &UserEvent) -> bool {
        matches!(event, UserEvent::Deleted)
    }

    async fn put(&self, tx: &mut Tx, event: &TopicEnvelope<UserTopic>) -> AppResult<()> {
        let user = &event.entity;
        sqlx::query!(
            r#"
            INSERT INTO note_search.author (id, display_name) VALUES ($1, $2)
            ON CONFLICT (id) DO UPDATE SET display_name = EXCLUDED.display_name
            "#,
            user.id,
            user.display_name,
        )
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }

    async fn remove(&self, tx: &mut Tx, key: &str) -> AppResult<()> {
        sqlx::query!("DELETE FROM note_search.author WHERE id = $1", id(key)?)
            .execute(conn(tx))
            .await
            .map_err(AppError::internal)?;
        Ok(())
    }
}
