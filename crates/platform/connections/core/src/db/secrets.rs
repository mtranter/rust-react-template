//! A Connection's secrets, read and written beside its row: a child has no repository of its own.

use crate::domain::{Connection, SealedSecret, Secret};
use app_db::conn;
use app_error::{AppError, AppResult};
use app_persistence::Tx;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::{BTreeMap, HashMap};

/// Each of `connections`' secrets, by name.
pub(super) async fn of(
    pool: &PgPool,
    connections: &[String],
) -> AppResult<HashMap<String, BTreeMap<String, Secret>>> {
    let rows = sqlx::query!(
        r#"
        SELECT connection_key, name, ciphertext, wrapped_key, key_version, set_at, set_by
        FROM connections.secret
        WHERE connection_key = ANY($1)
        "#,
        connections,
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::internal)?;

    let mut secrets: HashMap<String, BTreeMap<String, Secret>> = HashMap::new();
    for row in rows {
        secrets.entry(row.connection_key).or_default().insert(
            row.name,
            Secret {
                sealed: SealedSecret {
                    ciphertext: row.ciphertext,
                    wrapped_key: row.wrapped_key,
                    key_version: row.key_version,
                },
                set_at: row.set_at,
                set_by: row.set_by,
            },
        );
    }
    Ok(secrets)
}

/// Replaces the stored secrets with `connection`'s. Runs only once the Connection's own row
/// passed its version guard in the same `tx`, so no other writer races it.
pub(super) async fn replace(tx: &mut Tx, connection: &Connection) -> AppResult<()> {
    let key = connection.key().as_str();
    sqlx::query!(
        "DELETE FROM connections.secret WHERE connection_key = $1",
        key,
    )
    .execute(conn(tx))
    .await
    .map_err(AppError::internal)?;

    let mut names = Vec::new();
    let mut ciphertexts = Vec::new();
    let mut wrapped_keys = Vec::new();
    let mut key_versions = Vec::new();
    let mut set_ats: Vec<DateTime<Utc>> = Vec::new();
    let mut set_bys = Vec::new();
    for (name, secret) in connection.secrets() {
        names.push(name.clone());
        ciphertexts.push(secret.sealed.ciphertext.clone());
        wrapped_keys.push(secret.sealed.wrapped_key.clone());
        key_versions.push(secret.sealed.key_version.clone());
        set_ats.push(secret.set_at);
        set_bys.push(secret.set_by.clone());
    }
    sqlx::query!(
        r#"
        INSERT INTO connections.secret
            (connection_key, name, ciphertext, wrapped_key, key_version, set_at, set_by)
        SELECT $1, s.name, s.ciphertext, s.wrapped_key, s.key_version, s.set_at, s.set_by
        FROM unnest($2::text[], $3::bytea[], $4::bytea[], $5::text[], $6::timestamptz[], $7::text[])
            AS s (name, ciphertext, wrapped_key, key_version, set_at, set_by)
        "#,
        key,
        &names,
        &ciphertexts,
        &wrapped_keys,
        &key_versions,
        &set_ats,
        &set_bys,
    )
    .execute(conn(tx))
    .await
    .map_err(AppError::internal)?;
    Ok(())
}
