use super::secrets;
use crate::application::{
    ConnectionRepository, ConnectionSortKey, Expected, ListQuery, Listed, Tx, Version, Versioned,
};
use crate::domain::{Connection, ConnectionKey};
use app_authz::Rows;
use app_db::{Clause, Column, Columns, conn, push_list_clauses, push_scope};
use app_error::{AppError, AppResult};
use connections_api::Declaration;
use serde_json::Value;
use sqlx::{PgPool, QueryBuilder};
use std::collections::BTreeMap;

pub struct PgConnectionRepository {
    pool: PgPool,
}

impl PgConnectionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn sort_column(key: ConnectionSortKey) -> &'static str {
    match key {
        ConnectionSortKey::Key => "key",
        ConnectionSortKey::Label => "label",
    }
}

/// Where a Connection's attributes are, for a plan's filter to become a list's `WHERE` (ADR 0040).
pub struct ConnectionColumns;

impl Columns for ConnectionColumns {
    type Resource = Connection;

    fn column(attr: &str) -> Option<Column> {
        match attr {
            "key" => Some(Column::text("key")),
            _ => None,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ConnectionRow {
    key: String,
    declaration: Value,
    declared: bool,
    url: Option<String>,
    fields: Value,
    version: i64,
}

const SELECT: &str =
    "SELECT key, declaration, declared, url, fields, version FROM connections.connection";

impl PgConnectionRepository {
    /// With each row's secrets, read in one more query.
    async fn restore(&self, rows: Vec<ConnectionRow>) -> AppResult<Vec<Versioned<Connection>>> {
        let keys: Vec<String> = rows.iter().map(|r| r.key.clone()).collect();
        let mut secrets = secrets::of(&self.pool, &keys).await?;
        rows.into_iter()
            .map(|r| {
                let secrets = secrets.remove(&r.key).unwrap_or_default();
                let key = ConnectionKey::parse(&r.key).map_err(AppError::internal)?;
                let declaration: Declaration =
                    serde_json::from_value(r.declaration).map_err(AppError::internal)?;
                let fields: BTreeMap<String, String> =
                    serde_json::from_value(r.fields).map_err(AppError::internal)?;
                Ok(Versioned {
                    entity: Connection::restore(
                        key,
                        declaration,
                        r.declared,
                        r.url,
                        fields,
                        secrets,
                    ),
                    version: Version::from_i64(r.version),
                })
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl ConnectionRepository for PgConnectionRepository {
    async fn get(&self, key: &ConnectionKey) -> AppResult<Option<Versioned<Connection>>> {
        let row = sqlx::query_as!(
            ConnectionRow,
            r#"
            SELECT key, declaration, declared, url, fields, version
            FROM connections.connection WHERE key = $1
            "#,
            key.as_str(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;
        Ok(self.restore(row.into_iter().collect()).await?.pop())
    }

    async fn all(&self) -> AppResult<Vec<Versioned<Connection>>> {
        let rows = sqlx::query_as!(
            ConnectionRow,
            r#"
            SELECT key, declaration, declared, url, fields, version
            FROM connections.connection ORDER BY key
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::internal)?;
        self.restore(rows).await
    }

    async fn list(
        &self,
        query: &ListQuery<ConnectionSortKey>,
        rows: &Rows,
    ) -> AppResult<Listed<Versioned<Connection>>> {
        let mut sql = QueryBuilder::new(SELECT);
        sql.push(" WHERE declared");
        push_scope::<ConnectionColumns>(&mut sql, Clause::And, rows);
        push_list_clauses(&mut sql, query, sort_column, "key");
        let found: Vec<ConnectionRow> = sql
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;

        let total = match query.page {
            Some(_) => {
                let mut count =
                    QueryBuilder::new("SELECT COUNT(*) FROM connections.connection WHERE declared");
                push_scope::<ConnectionColumns>(&mut count, Clause::And, rows);
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
            items: self.restore(found).await?,
            total,
        })
    }

    async fn save(
        &self,
        tx: &mut Tx,
        connection: &Connection,
        expected: Expected,
    ) -> AppResult<Version> {
        let declaration =
            serde_json::to_value(connection.declaration()).map_err(AppError::internal)?;
        let fields = serde_json::to_value(connection.fields()).map_err(AppError::internal)?;
        let version = match expected {
            // A duplicate key is an internal error: `migrate` declares each key once.
            Expected::New => {
                let version = sqlx::query_scalar!(
                    r#"
                    INSERT INTO connections.connection
                        (key, label, declaration, declared, url, fields, version)
                    VALUES ($1, $2, $3, $4, $5, $6, 1)
                    RETURNING version
                    "#,
                    connection.key().as_str(),
                    connection.label(),
                    declaration,
                    connection.declared(),
                    connection.url(),
                    fields,
                )
                .fetch_one(conn(tx))
                .await
                .map_err(AppError::internal)?;
                Version::from_i64(version)
            }
            // No row updated ⇒ the Connection changed since the caller read it.
            Expected::Version(expected) => {
                let updated = sqlx::query_scalar!(
                    r#"
                    UPDATE connections.connection
                    SET label = $2, declaration = $3, declared = $4, url = $5, fields = $6,
                        version = version + 1
                    WHERE key = $1 AND version = $7
                    RETURNING version
                    "#,
                    connection.key().as_str(),
                    connection.label(),
                    declaration,
                    connection.declared(),
                    connection.url(),
                    fields,
                    expected.as_i64(),
                )
                .fetch_optional(conn(tx))
                .await
                .map_err(AppError::internal)?;
                updated.map(Version::from_i64).ok_or_else(|| {
                    AppError::precondition_failed("the connection was modified by someone else")
                })?
            }
        };
        secrets::replace(tx, connection).await?;
        Ok(version)
    }
}
