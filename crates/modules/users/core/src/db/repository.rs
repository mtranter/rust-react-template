use crate::application::{
    Expected, ListQuery, Listed, UserRepository, UserSortKey, Version, Versioned,
};
use crate::domain::{Email, User, UserId};
use app_db::{map_db_error, push_list_clauses};
use app_error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, QueryBuilder};
use uuid::Uuid;

pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_sqlx(err: sqlx::Error) -> AppError {
    map_db_error(err, |constraint| match constraint {
        // Postgres' name for the inline `UNIQUE` on `email` (0001_init.sql).
        "account_email_key" => Some(AppError::conflict("a user with that email already exists")),
        _ => None,
    })
}

fn sort_column(key: UserSortKey) -> &'static str {
    match key {
        UserSortKey::Email => "email",
        UserSortKey::DisplayName => "display_name",
        UserSortKey::CreatedAt => "created_at",
    }
}

/// Lists build their SQL at runtime (the sort varies), so they can't use `query!`.
#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    display_name: String,
    created_at: DateTime<Utc>,
    version: i64,
}

impl UserRow {
    fn restore(r: Self) -> Versioned<User> {
        Versioned {
            entity: User::restore(r.id.into(), r.email, r.display_name, r.created_at),
            version: Version::from_i64(r.version),
        }
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

    async fn list(&self, query: &ListQuery<UserSortKey>) -> AppResult<Listed<Versioned<User>>> {
        let mut sql = QueryBuilder::new(
            "SELECT id, email, display_name, created_at, version FROM users.account",
        );
        push_list_clauses(&mut sql, query, sort_column, "created_at DESC, id");
        let rows: Vec<UserRow> = sql
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?;

        let total = match query.page {
            Some(_) => sqlx::query_scalar!(r#"SELECT COUNT(*) FROM users.account"#)
                .fetch_one(&self.pool)
                .await
                .map_err(map_sqlx)?,
            None => None,
        };

        Ok(Listed {
            items: rows.into_iter().map(UserRow::restore).collect(),
            total,
        })
    }

    async fn save(&self, user: &User, expected: Expected) -> AppResult<Version> {
        match expected {
            // A duplicate email maps to a conflict via `map_sqlx`.
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
            // No row updated ⇒ the account changed since the caller read it.
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
