//! What each Connection last tested as: beside its row, never part of its version.

use crate::domain::{ConnectionKey, Health, Outcome};
use app_error::{AppError, AppResult};
use app_persistence::Version;
use sqlx::PgPool;
use std::collections::HashMap;

pub(super) async fn of(
    pool: &PgPool,
    keys: &[String],
) -> AppResult<HashMap<String, (Version, Health)>> {
    let rows = sqlx::query!(
        r#"
        SELECT connection_key, version, ok, error, tested_at
        FROM connections.health
        WHERE connection_key = ANY($1)
        "#,
        keys,
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::internal)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let outcome = if row.ok {
                Outcome::Ok
            } else {
                Outcome::Failing(row.error.unwrap_or_default())
            };
            (
                row.connection_key,
                (
                    Version::from_i64(row.version),
                    Health {
                        outcome,
                        tested_at: row.tested_at,
                    },
                ),
            )
        })
        .collect())
}

pub(super) async fn record(
    pool: &PgPool,
    key: &ConnectionKey,
    version: Version,
    health: &Health,
) -> AppResult<()> {
    let (ok, error) = match &health.outcome {
        Outcome::Ok => (true, None),
        Outcome::Failing(error) => (false, Some(error.as_str())),
    };
    // An observation, not an aggregate: the last one wins.
    sqlx::query!(
        r#"
        INSERT INTO connections.health (connection_key, version, ok, error, tested_at)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (connection_key) DO UPDATE
        SET version = $2, ok = $3, error = $4, tested_at = $5
        "#,
        key.as_str(),
        version.as_i64(),
        ok,
        error,
        health.tested_at,
    )
    .execute(pool)
    .await
    .map_err(AppError::internal)?;
    Ok(())
}
