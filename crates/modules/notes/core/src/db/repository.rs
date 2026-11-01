use crate::application::{
    Expected, ListQuery, Listed, NoteRepository, NoteSortKey, Version, Versioned,
};
use crate::domain::{Note, NoteId};
use app_db::push_list_clauses;
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
    version: i64,
}

impl NoteRow {
    fn restore(r: Self) -> Versioned<Note> {
        Versioned {
            entity: Note::restore(
                r.id.into(),
                r.author_id.into(),
                r.title,
                r.body,
                r.created_at,
            ),
            version: Version::from_i64(r.version),
        }
    }
}

#[async_trait::async_trait]
impl NoteRepository for PgNoteRepository {
    async fn get(&self, id: NoteId) -> AppResult<Option<Versioned<Note>>> {
        let rec = sqlx::query!(
            r#"SELECT id, author_id, title, body, created_at, version FROM notes.note WHERE id = $1"#,
            id.as_uuid(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;

        Ok(rec.map(|r| Versioned {
            entity: Note::restore(
                r.id.into(),
                r.author_id.into(),
                r.title,
                r.body,
                r.created_at,
            ),
            version: Version::from_i64(r.version),
        }))
    }

    async fn list(&self, query: &ListQuery<NoteSortKey>) -> AppResult<Listed<Versioned<Note>>> {
        let mut sql = QueryBuilder::new(
            "SELECT id, author_id, title, body, created_at, version FROM notes.note",
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
            items: rows.into_iter().map(NoteRow::restore).collect(),
            total,
        })
    }

    async fn save(&self, note: &Note, expected: Expected) -> AppResult<Version> {
        match expected {
            // A duplicate id is an internal error: ids are minted, not client-chosen.
            Expected::New => {
                let version = sqlx::query_scalar!(
                    r#"
                    INSERT INTO notes.note (id, author_id, title, body, created_at, version)
                    VALUES ($1, $2, $3, $4, $5, 1)
                    RETURNING version
                    "#,
                    note.id().as_uuid(),
                    note.author_id().as_uuid(),
                    note.title(),
                    note.body(),
                    note.created_at(),
                )
                .fetch_one(&self.pool)
                .await
                .map_err(AppError::internal)?;
                Ok(Version::from_i64(version))
            }
            // No row updated ⇒ the note changed since the caller read it.
            Expected::Version(expected) => {
                let updated = sqlx::query_scalar!(
                    r#"
                    UPDATE notes.note
                    SET title = $2, body = $3, version = version + 1
                    WHERE id = $1 AND version = $4
                    RETURNING version
                    "#,
                    note.id().as_uuid(),
                    note.title(),
                    note.body(),
                    expected.as_i64(),
                )
                .fetch_optional(&self.pool)
                .await
                .map_err(AppError::internal)?;

                updated.map(Version::from_i64).ok_or_else(|| {
                    AppError::precondition_failed("the note was modified by someone else")
                })
            }
        }
    }
}
