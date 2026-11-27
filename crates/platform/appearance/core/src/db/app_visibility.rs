use crate::application::{AppVisibilityRepository, Expected, Tx, Version, Versioned};
use crate::domain::AppVisibility;
use app_db::{conn, map_db_error};
use app_error::{AppError, AppResult};
use sqlx::PgPool;

pub struct PgAppVisibilityRepository {
    pool: PgPool,
}

impl PgAppVisibilityRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl AppVisibilityRepository for PgAppVisibilityRepository {
    async fn get(&self) -> AppResult<Option<Versioned<AppVisibility>>> {
        let row = sqlx::query!(
            r#"
            SELECT hidden, updated_at, updated_by, version
            FROM appearance.app_visibility WHERE key = 'org'
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;
        Ok(row.map(|r| Versioned {
            entity: AppVisibility::restore(r.hidden, r.updated_at, r.updated_by),
            version: Version::from_i64(r.version),
        }))
    }

    async fn save(
        &self,
        tx: &mut Tx,
        visibility: &AppVisibility,
        expected: Expected,
    ) -> AppResult<Version> {
        let hidden: Vec<String> = visibility.hidden().iter().cloned().collect();
        let saved = match expected {
            Expected::New => sqlx::query_scalar!(
                r#"
                INSERT INTO appearance.app_visibility
                    (key, hidden, updated_at, updated_by, version)
                VALUES ('org', $1, $2, $3, 1)
                RETURNING version
                "#,
                &hidden,
                visibility.updated_at(),
                visibility.updated_by(),
            )
            .fetch_one(conn(tx))
            .await
            .map(Some)
            // Someone saved first since the caller read none.
            .map_err(|err| {
                map_db_error(err, |constraint| {
                    (constraint == "app_visibility_pkey")
                        .then(crate::application::app_visibility_stale_error)
                })
            })?,
            Expected::Version(expected) => sqlx::query_scalar!(
                r#"
                UPDATE appearance.app_visibility
                SET hidden = $1, updated_at = $2, updated_by = $3, version = version + 1
                WHERE key = 'org' AND version = $4
                RETURNING version
                "#,
                &hidden,
                visibility.updated_at(),
                visibility.updated_by(),
                expected.as_i64(),
            )
            .fetch_optional(conn(tx))
            .await
            .map_err(AppError::internal)?,
        };
        saved
            .map(Version::from_i64)
            .ok_or_else(crate::application::app_visibility_stale_error)
    }
}
