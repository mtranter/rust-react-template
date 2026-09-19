//! Postgres-backed [`UserRepository`].

use crate::application::{Expected, UserRepository, Version, Versioned};
use crate::domain::{Email, User, UserId};
use app_error::{AppError, AppResult};
use sqlx::PgPool;

/// Postgres-backed [`UserRepository`]. Holds a cheaply-cloneable pool handle.
pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Map driver errors onto the canonical taxonomy. Unique violations become
/// conflicts; everything unexpected is an opaque internal error.
fn map_sqlx(err: sqlx::Error) -> AppError {
    match &err {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::conflict("a user with that email already exists")
        }
        _ => AppError::internal(err),
    }
}

#[async_trait::async_trait]
impl UserRepository for PgUserRepository {
    async fn get(&self, id: UserId) -> AppResult<Option<Versioned<User>>> {
        let rec = sqlx::query!(
            r#"SELECT id, email, display_name, created_at, version FROM users.account WHERE id = $1"#,
            id.as_uuid(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(rec.map(|r| Versioned {
            entity: User::restore(r.id.into(), r.email, r.display_name, r.created_at),
            version: Version::from_i64(r.version),
        }))
    }

    async fn find_by_email(&self, email: &Email) -> AppResult<Option<User>> {
        let rec = sqlx::query!(
            r#"SELECT id, email, display_name, created_at FROM users.account WHERE email = $1"#,
            email.as_str(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(rec.map(|r| User::restore(r.id.into(), r.email, r.display_name, r.created_at)))
    }

    async fn list(&self, limit: i64, offset: i64) -> AppResult<(Vec<Versioned<User>>, i64)> {
        let rows = sqlx::query!(
            r#"
            SELECT id, email, display_name, created_at, version
            FROM users.account
            ORDER BY created_at DESC, id
            LIMIT $1 OFFSET $2
            "#,
            limit,
            offset,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        let total = sqlx::query_scalar!(r#"SELECT COUNT(*) FROM users.account"#)
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx)?
            .unwrap_or(0);

        let users = rows
            .into_iter()
            .map(|r| Versioned {
                entity: User::restore(r.id.into(), r.email, r.display_name, r.created_at),
                version: Version::from_i64(r.version),
            })
            .collect();

        Ok((users, total))
    }

    async fn save(&self, user: &User, expected: Expected) -> AppResult<Version> {
        match expected {
            // A fresh account: plain insert at the first version. A duplicate
            // email still maps to a conflict via `map_sqlx`.
            Expected::New => {
                let version = sqlx::query_scalar!(
                    r#"
                    INSERT INTO users.account (id, email, display_name, created_at, version)
                    VALUES ($1, $2, $3, $4, 1)
                    RETURNING version
                    "#,
                    user.id().as_uuid(),
                    user.email().as_str(),
                    user.display_name(),
                    user.created_at(),
                )
                .fetch_one(&self.pool)
                .await
                .map_err(map_sqlx)?;
                Ok(Version::from_i64(version))
            }
            // A conditional update: bump the version only if it still matches what
            // the caller read. No row updated ⇒ the account changed underneath them.
            Expected::Version(expected) => {
                let updated = sqlx::query_scalar!(
                    r#"
                    UPDATE users.account
                    SET email = $2, display_name = $3, version = version + 1
                    WHERE id = $1 AND version = $4
                    RETURNING version
                    "#,
                    user.id().as_uuid(),
                    user.email().as_str(),
                    user.display_name(),
                    expected.as_i64(),
                )
                .fetch_optional(&self.pool)
                .await
                .map_err(map_sqlx)?;

                updated.map(Version::from_i64).ok_or_else(|| {
                    AppError::precondition_failed("the account was modified elsewhere")
                })
            }
        }
    }
}
