//! Postgres-backed [`NoteRepository`].

use crate::application::{Expected, NoteRepository, Version, Versioned};
use crate::domain::{Note, NoteId};
use app_error::{AppError, AppResult};
use sqlx::PgPool;

pub struct PgNoteRepository {
    pool: PgPool,
}

impl PgNoteRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_sqlx(err: sqlx::Error) -> AppError {
    AppError::internal(err)
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
        .map_err(map_sqlx)?;

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

    async fn list(&self, limit: i64, offset: i64) -> AppResult<(Vec<Versioned<Note>>, i64)> {
        let rows = sqlx::query!(
            r#"
            SELECT id, author_id, title, body, created_at, version
            FROM notes.note
            ORDER BY created_at DESC, id
            LIMIT $1 OFFSET $2
            "#,
            limit,
            offset,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        let total = sqlx::query_scalar!(r#"SELECT COUNT(*) FROM notes.note"#)
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx)?
            .unwrap_or(0);

        let notes = rows
            .into_iter()
            .map(|r| Versioned {
                entity: Note::restore(
                    r.id.into(),
                    r.author_id.into(),
                    r.title,
                    r.body,
                    r.created_at,
                ),
                version: Version::from_i64(r.version),
            })
            .collect();

        Ok((notes, total))
    }

    async fn save(&self, note: &Note, expected: Expected) -> AppResult<Version> {
        match expected {
            // A fresh aggregate: plain insert at the first version. A duplicate id
            // is an unexpected internal error (ids are minted, not client-chosen).
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
                .map_err(map_sqlx)?;
                Ok(Version::from_i64(version))
            }
            // A conditional update: bump the version only if it still matches what
            // the caller read. No row updated ⇒ the note changed underneath them.
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
                .map_err(map_sqlx)?;

                updated.map(Version::from_i64).ok_or_else(|| {
                    AppError::precondition_failed("the note was modified by someone else")
                })
            }
        }
    }
}
