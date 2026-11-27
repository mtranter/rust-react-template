use crate::application::{Expected, ThemeRepository, Tx, Version, Versioned};
use crate::domain::{Overrides, Theme};
use app_db::{conn, map_db_error};
use app_error::{AppError, AppResult};
use sqlx::PgPool;

pub struct PgThemeRepository {
    pool: PgPool,
}

impl PgThemeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn overrides(value: serde_json::Value) -> AppResult<Overrides> {
    serde_json::from_value(value).map_err(AppError::internal)
}

fn json(overrides: &Overrides) -> AppResult<serde_json::Value> {
    serde_json::to_value(overrides).map_err(AppError::internal)
}

#[async_trait::async_trait]
impl ThemeRepository for PgThemeRepository {
    async fn get(&self) -> AppResult<Option<Versioned<Theme>>> {
        let row = sqlx::query!(
            r#"
            SELECT light, dark, shared, updated_at, updated_by, version
            FROM appearance.theme WHERE key = 'org'
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;
        row.map(|r| {
            Ok(Versioned {
                entity: Theme::restore(
                    overrides(r.light)?,
                    overrides(r.dark)?,
                    overrides(r.shared)?,
                    r.updated_at,
                    r.updated_by,
                ),
                version: Version::from_i64(r.version),
            })
        })
        .transpose()
    }

    async fn save(&self, tx: &mut Tx, theme: &Theme, expected: Expected) -> AppResult<Version> {
        let (light, dark, shared) = (
            json(theme.light())?,
            json(theme.dark())?,
            json(theme.shared())?,
        );
        let saved = match expected {
            Expected::New => sqlx::query_scalar!(
                r#"
                INSERT INTO appearance.theme
                    (key, light, dark, shared, updated_at, updated_by, version)
                VALUES ('org', $1, $2, $3, $4, $5, 1)
                RETURNING version
                "#,
                light,
                dark,
                shared,
                theme.updated_at(),
                theme.updated_by(),
            )
            .fetch_one(conn(tx))
            .await
            .map(Some)
            // Someone saved the first Theme since the caller read none.
            .map_err(|err| {
                map_db_error(err, |constraint| {
                    (constraint == "theme_pkey").then(crate::application::stale_error)
                })
            })?,
            Expected::Version(expected) => sqlx::query_scalar!(
                r#"
                UPDATE appearance.theme
                SET light = $1, dark = $2, shared = $3, updated_at = $4, updated_by = $5,
                    version = version + 1
                WHERE key = 'org' AND version = $6
                RETURNING version
                "#,
                light,
                dark,
                shared,
                theme.updated_at(),
                theme.updated_by(),
                expected.as_i64(),
            )
            .fetch_optional(conn(tx))
            .await
            .map_err(AppError::internal)?,
        };
        saved
            .map(Version::from_i64)
            .ok_or_else(crate::application::stale_error)
    }
}
