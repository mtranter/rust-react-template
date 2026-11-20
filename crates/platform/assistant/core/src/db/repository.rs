use super::messages;
use crate::application::{
    ConversationRepository, ConversationSortKey, ConversationSummary, Expected, ListQuery, Listed,
    Tx, Version, Versioned,
};
use crate::domain::{Conversation, ConversationId};
use app_authz::Rows;
use app_db::{Clause, Column, Columns, conn, push_list_clauses, push_scope};
use app_error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, QueryBuilder};
use uuid::Uuid;

const STALE: &str = "the conversation was changed meanwhile";

pub struct PgConversationRepository {
    pool: PgPool,
}

impl PgConversationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn sort_column(key: ConversationSortKey) -> &'static str {
    match key {
        ConversationSortKey::CreatedAt => "created_at",
    }
}

/// Where a conversation's attributes are, for a plan's filter to become a list's `WHERE`
/// (ADR 0040).
pub struct ConversationColumns;

impl Columns for ConversationColumns {
    type Resource = Conversation;

    fn column(attr: &str) -> Option<Column> {
        match attr {
            "ownerId" => Some(Column::text("owner_id")),
            _ => None,
        }
    }
}

struct ConversationRow {
    id: Uuid,
    owner_id: String,
    title: String,
    created_at: DateTime<Utc>,
    version: i64,
}

/// Lists build their SQL at runtime (the sort varies), so they can't use `query!`.
#[derive(sqlx::FromRow)]
struct SummaryRow {
    id: Uuid,
    title: String,
    created_at: DateTime<Utc>,
    version: i64,
}

#[async_trait::async_trait]
impl ConversationRepository for PgConversationRepository {
    async fn get(&self, id: ConversationId) -> AppResult<Option<Versioned<Conversation>>> {
        // One snapshot for the row and its children: a turn saves while its owner reads.
        let mut snapshot = self.pool.begin().await.map_err(AppError::internal)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *snapshot)
            .await
            .map_err(AppError::internal)?;
        let Some(row) = sqlx::query_as!(
            ConversationRow,
            r#"
            SELECT id, owner_id, title, created_at, version
            FROM assistant.conversation WHERE id = $1
            "#,
            id.as_uuid(),
        )
        .fetch_optional(&mut *snapshot)
        .await
        .map_err(AppError::internal)?
        else {
            return Ok(None);
        };

        let messages = messages::of(&mut snapshot, &[row.id])
            .await?
            .remove(&row.id)
            .unwrap_or_default();
        Ok(Some(Versioned {
            entity: Conversation::restore(
                row.id.into(),
                row.owner_id,
                row.title,
                row.created_at,
                messages,
            ),
            version: Version::from_i64(row.version),
        }))
    }

    async fn list(
        &self,
        query: &ListQuery<ConversationSortKey>,
        rows: &Rows,
    ) -> AppResult<Listed<Versioned<ConversationSummary>>> {
        let mut sql =
            QueryBuilder::new("SELECT id, title, created_at, version FROM assistant.conversation");
        push_scope::<ConversationColumns>(&mut sql, Clause::Where, rows);
        push_list_clauses(&mut sql, query, sort_column, "created_at DESC, id");
        let found: Vec<SummaryRow> = sql
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;

        let total = match query.page {
            Some(_) => {
                let mut count = QueryBuilder::new("SELECT COUNT(*) FROM assistant.conversation");
                push_scope::<ConversationColumns>(&mut count, Clause::Where, rows);
                Some(
                    count
                        .build_query_scalar()
                        .fetch_one(&self.pool)
                        .await
                        .map_err(AppError::internal)?,
                )
            }
            None => None,
        };

        Ok(Listed {
            items: found
                .into_iter()
                .map(|row| Versioned {
                    entity: ConversationSummary {
                        id: row.id.into(),
                        title: row.title,
                        created_at: row.created_at,
                    },
                    version: Version::from_i64(row.version),
                })
                .collect(),
            total,
        })
    }

    async fn save(
        &self,
        tx: &mut Tx,
        conversation: &Conversation,
        expected: Expected,
    ) -> AppResult<Version> {
        let version = match expected {
            // A duplicate id is an internal error: ids are minted, not client-chosen.
            Expected::New => sqlx::query_scalar!(
                r#"
                INSERT INTO assistant.conversation (id, owner_id, title, created_at, version)
                VALUES ($1, $2, $3, $4, 1)
                RETURNING version
                "#,
                conversation.id().as_uuid(),
                conversation.owner_id(),
                conversation.title(),
                conversation.created_at(),
            )
            .fetch_one(conn(tx))
            .await
            .map_err(AppError::internal)?,
            // No row updated ⇒ the conversation changed since the caller read it.
            Expected::Version(expected) => sqlx::query_scalar!(
                r#"
                UPDATE assistant.conversation
                SET title = $2, version = version + 1
                WHERE id = $1 AND version = $3
                RETURNING version
                "#,
                conversation.id().as_uuid(),
                conversation.title(),
                expected.as_i64(),
            )
            .fetch_optional(conn(tx))
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::precondition_failed(STALE))?,
        };
        messages::replace(tx, conversation.id().as_uuid(), conversation.messages()).await?;
        Ok(Version::from_i64(version))
    }

    async fn delete(&self, tx: &mut Tx, id: ConversationId, expected: Version) -> AppResult<()> {
        sqlx::query_scalar!(
            "DELETE FROM assistant.conversation WHERE id = $1 AND version = $2 RETURNING id",
            id.as_uuid(),
            expected.as_i64(),
        )
        .fetch_optional(conn(tx))
        .await
        .map_err(AppError::internal)?
        .map(|_| ())
        .ok_or_else(|| AppError::precondition_failed(STALE))
    }
}
