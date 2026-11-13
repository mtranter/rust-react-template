use super::items;
use crate::application::{
    Expected, ListQuery, Listed, NoteRepository, NoteSortKey, Tx, Version, Versioned,
};
use crate::domain::{AuthorId, Item, Note, NoteId};
use app_db::{conn, push_list_clauses};
use app_error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, QueryBuilder};
use uuid::Uuid;

pub struct PgNoteRepository {
    pool: PgPool,
}

impl PgNoteRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn sort_column(key: NoteSortKey) -> &'static str {
    match key {
        NoteSortKey::Title => "title",
        NoteSortKey::CreatedAt => "created_at",
    }
}

/// Lists build their SQL at runtime (the sort varies), so they can't use `query!`.
#[derive(sqlx::FromRow)]
struct NoteRow {
    id: Uuid,
    author_id: Uuid,
    title: String,
    body: String,
    created_at: DateTime<Utc>,
    archived_at: Option<DateTime<Utc>>,
    version: i64,
}

impl NoteRow {
    /// With the rows' items, read in one more query.
    async fn restore_all(pool: &PgPool, rows: Vec<Self>) -> AppResult<Vec<Versioned<Note>>> {
        let ids: Vec<Uuid> = rows.iter().map(|row| row.id).collect();
        let mut items = items::of(pool, &ids).await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let items = items.remove(&row.id).unwrap_or_default();
                row.restore(items)
            })
            .collect())
    }

    fn restore(self, items: Vec<Item>) -> Versioned<Note> {
        let r = self;
        Versioned {
            entity: Note::restore(
                r.id.into(),
                r.author_id.into(),
                r.title,
                r.body,
                r.created_at,
                r.archived_at,
                items,
            ),
            version: Version::from_i64(r.version),
        }
    }
}

#[async_trait::async_trait]
impl NoteRepository for PgNoteRepository {
    async fn get(&self, id: NoteId) -> AppResult<Option<Versioned<Note>>> {
        let row = sqlx::query_as!(
            NoteRow,
            r#"
            SELECT id, author_id, title, body, created_at, archived_at, version
            FROM notes.note WHERE id = $1
            "#,
            id.as_uuid(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;

        Ok(NoteRow::restore_all(&self.pool, row.into_iter().collect())
            .await?
            .pop())
    }

    async fn list(&self, query: &ListQuery<NoteSortKey>) -> AppResult<Listed<Versioned<Note>>> {
        let mut sql = QueryBuilder::new(
            "SELECT id, author_id, title, body, created_at, archived_at, version FROM notes.note",
        );
        push_list_clauses(&mut sql, query, sort_column, "created_at DESC, id");
        let rows: Vec<NoteRow> = sql
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;

        let total = match query.page {
            Some(_) => sqlx::query_scalar!(r#"SELECT COUNT(*) FROM notes.note"#)
                .fetch_one(&self.pool)
                .await
                .map_err(AppError::internal)?,
            None => None,
        };

        Ok(Listed {
            items: NoteRow::restore_all(&self.pool, rows).await?,
            total,
        })
    }

    async fn unarchived_by(&self, author: AuthorId) -> AppResult<Vec<Versioned<Note>>> {
        let rows = sqlx::query_as!(
            NoteRow,
            r#"
            SELECT id, author_id, title, body, created_at, archived_at, version
            FROM notes.note
            WHERE author_id = $1 AND archived_at IS NULL
            ORDER BY created_at, id
            "#,
            author.as_uuid(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::internal)?;

        NoteRow::restore_all(&self.pool, rows).await
    }

    async fn save(&self, tx: &mut Tx, note: &Note, expected: Expected) -> AppResult<Version> {
        match expected {
            // A duplicate id is an internal error: ids are minted, not client-chosen.
            Expected::New => {
                let version = sqlx::query_scalar!(
                    r#"
                    INSERT INTO notes.note (id, author_id, title, body, created_at, archived_at, version)
                    VALUES ($1, $2, $3, $4, $5, $6, 1)
                    RETURNING version
                    "#,
                    note.id().as_uuid(),
                    note.author_id().as_uuid(),
                    note.title(),
                    note.body(),
                    note.created_at(),
                    note.archived_at(),
                )
                .fetch_one(conn(tx))
                .await
                .map_err(AppError::internal)?;
                items::replace(tx, note).await?;
                Ok(Version::from_i64(version))
            }
            // No row updated ⇒ the note changed since the caller read it.
            Expected::Version(expected) => {
                let updated = sqlx::query_scalar!(
                    r#"
                    UPDATE notes.note
                    SET title = $2, body = $3, archived_at = $4, version = version + 1
                    WHERE id = $1 AND version = $5
                    RETURNING version
                    "#,
                    note.id().as_uuid(),
                    note.title(),
                    note.body(),
                    note.archived_at(),
                    expected.as_i64(),
                )
                .fetch_optional(conn(tx))
                .await
                .map_err(AppError::internal)?;

                let version = updated.map(Version::from_i64).ok_or_else(|| {
                    AppError::precondition_failed("the note was modified by someone else")
                })?;
                items::replace(tx, note).await?;
                Ok(version)
            }
        }
    }

    async fn delete(&self, tx: &mut Tx, id: NoteId, expected: Version) -> AppResult<()> {
        let deleted = sqlx::query_scalar!(
            r#"DELETE FROM notes.note WHERE id = $1 AND version = $2 RETURNING id"#,
            id.as_uuid(),
            expected.as_i64(),
        )
        .fetch_optional(conn(tx))
        .await
        .map_err(AppError::internal)?;

        deleted
            .map(|_| ())
            .ok_or_else(|| AppError::precondition_failed("the note was modified by someone else"))
    }
}
