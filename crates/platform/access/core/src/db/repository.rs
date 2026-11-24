use crate::application::{
    GrantFilter, GrantRepository, GrantSortKey, ListQuery, Listed, Tx, Version, Versioned,
    scope_name,
};
use crate::domain::{Grant, GrantId, GrantedBy, Scope};
use app_authz::Rows;
use app_db::{Clause, Column, Columns, conn, map_db_error, push_list_clauses, push_scope};
use app_error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

pub struct PgGrantRepository {
    pool: PgPool,
}

impl PgGrantRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_sqlx(err: sqlx::Error) -> AppError {
    map_db_error(err, |constraint| match constraint {
        "grant_once" => Some(AppError::conflict(
            "that role is granted that action within that scope already",
        )),
        "grant_role_exists" => Some(AppError::invalid_field(
            "role",
            "no role has that name: create it first",
        )),
        _ => None,
    })
}

/// Where a grant's attributes are, for a plan's filter to become a list's `WHERE` (ADR 0040).
pub struct GrantColumns;

impl Columns for GrantColumns {
    type Resource = Grant;

    fn column(_: &str) -> Option<Column> {
        None
    }
}

fn sort_column(key: GrantSortKey) -> &'static str {
    match key {
        GrantSortKey::Role => "role",
        GrantSortKey::Resource => "resource",
        GrantSortKey::Action => "action",
        GrantSortKey::GrantedAt => "granted_at",
    }
}

fn scope_of(stored: String) -> Scope {
    if stored == "any" {
        Scope::Any
    } else {
        Scope::Named(stored)
    }
}

fn rule(role: String, resource: String, action: String, scope: String) -> app_authz::Grant {
    app_authz::Grant {
        role,
        resource,
        action,
        scope: scope_of(scope),
    }
}

/// Lists build their SQL at runtime (the sort varies), so they can't use `query!`.
#[derive(sqlx::FromRow)]
struct GrantRow {
    id: Uuid,
    role: String,
    resource: String,
    action: String,
    scope: String,
    granted_by: Option<String>,
    granted_at: DateTime<Utc>,
    version: i64,
}

impl GrantRow {
    fn restore(self) -> Versioned<Grant> {
        Versioned {
            entity: Grant::restore(
                self.id.into(),
                rule(self.role, self.resource, self.action, self.scope),
                self.granted_by.map_or(GrantedBy::System, GrantedBy::User),
                self.granted_at,
            ),
            version: Version::from_i64(self.version),
        }
    }
}

const COLUMNS: &str =
    "SELECT id, role, resource, action, scope, granted_by, granted_at, version FROM access.grant";

fn push_filter(sql: &mut QueryBuilder<Postgres>, filter: &GrantFilter) {
    sql.push(" WHERE TRUE");
    for (column, value) in [
        ("role", &filter.role),
        ("resource", &filter.resource),
        ("action", &filter.action),
    ] {
        if let Some(value) = value {
            sql.push(format_args!(" AND {column} = "))
                .push_bind(value.clone());
        }
    }
}

#[async_trait::async_trait]
impl GrantRepository for PgGrantRepository {
    async fn get(&self, id: GrantId) -> AppResult<Option<Versioned<Grant>>> {
        let found = sqlx::query_as!(
            GrantRow,
            r#"SELECT id, role, resource, action, scope, granted_by, granted_at, version
               FROM access.grant WHERE id = $1"#,
            id.as_uuid(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;
        Ok(found.map(GrantRow::restore))
    }

    async fn list(
        &self,
        filter: &GrantFilter,
        query: &ListQuery<GrantSortKey>,
        rows: &Rows,
    ) -> AppResult<Listed<Versioned<Grant>>> {
        let mut sql = QueryBuilder::new(COLUMNS);
        push_filter(&mut sql, filter);
        push_scope::<GrantColumns>(&mut sql, Clause::And, rows);
        push_list_clauses(
            &mut sql,
            query,
            sort_column,
            "resource, action, role, scope, id",
        );
        let found: Vec<GrantRow> = sql
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;

        let total = match query.page {
            Some(_) => {
                let mut count = QueryBuilder::new("SELECT COUNT(*) FROM access.grant");
                push_filter(&mut count, filter);
                push_scope::<GrantColumns>(&mut count, Clause::And, rows);
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
            items: found.into_iter().map(GrantRow::restore).collect(),
            total,
        })
    }

    async fn all(&self) -> AppResult<Vec<Grant>> {
        let found = sqlx::query_as!(
            GrantRow,
            r#"SELECT id, role, resource, action, scope, granted_by, granted_at, version
               FROM access.grant"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::internal)?;
        Ok(found.into_iter().map(|row| row.restore().entity).collect())
    }

    async fn all_versioned(&self, tx: &mut Tx) -> AppResult<Vec<Versioned<Grant>>> {
        let found = sqlx::query_as!(
            GrantRow,
            r#"SELECT id, role, resource, action, scope, granted_by, granted_at, version
               FROM access.grant ORDER BY resource, action, role, scope"#,
        )
        .fetch_all(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(found.into_iter().map(GrantRow::restore).collect())
    }

    async fn insert(&self, tx: &mut Tx, grant: &Grant) -> AppResult<Version> {
        let version = sqlx::query_scalar!(
            r#"
            INSERT INTO access.grant
                (id, role, resource, action, scope, granted_by, granted_at, version)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 1)
            RETURNING version
            "#,
            grant.id().as_uuid(),
            grant.role(),
            grant.resource(),
            grant.action(),
            scope_name(grant.scope()),
            grant.granted_by().subject(),
            grant.granted_at(),
        )
        .fetch_one(conn(tx))
        .await
        .map_err(map_sqlx)?;
        Ok(Version::from_i64(version))
    }

    async fn exists(&self, tx: &mut Tx, rule: &app_authz::Grant) -> AppResult<bool> {
        let found = sqlx::query_scalar!(
            r#"SELECT EXISTS (
                 SELECT 1 FROM access.grant
                 WHERE role = $1 AND resource = $2 AND action = $3 AND scope = $4
               ) AS "found!""#,
            rule.role,
            rule.resource,
            rule.action,
            scope_name(&rule.scope),
        )
        .fetch_one(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(found)
    }

    async fn delete(&self, tx: &mut Tx, id: GrantId, expected: Version) -> AppResult<()> {
        let deleted = sqlx::query!(
            r#"DELETE FROM access.grant WHERE id = $1 AND version = $2"#,
            id.as_uuid(),
            expected.as_i64(),
        )
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        if deleted.rows_affected() == 0 {
            return Err(AppError::precondition_failed(
                "the grant was revoked meanwhile",
            ));
        }
        Ok(())
    }

    async fn revision(&self) -> AppResult<i64> {
        sqlx::query_scalar!(r#"SELECT revision FROM access.revision"#)
            .fetch_one(&self.pool)
            .await
            .map_err(AppError::internal)
    }

    async fn bump_revision(&self, tx: &mut Tx) -> AppResult<i64> {
        sqlx::query_scalar!(
            r#"UPDATE access.revision SET revision = revision + 1 RETURNING revision"#
        )
        .fetch_one(conn(tx))
        .await
        .map_err(AppError::internal)
    }

    async fn applied_defaults(&self, tx: &mut Tx) -> AppResult<Vec<(String, app_authz::Grant)>> {
        let found = sqlx::query!(
            r#"SELECT module, role, resource, action, scope FROM access.applied_default"#
        )
        .fetch_all(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(found
            .into_iter()
            .map(|r| (r.module, rule(r.role, r.resource, r.action, r.scope)))
            .collect())
    }

    async fn record_default(
        &self,
        tx: &mut Tx,
        module: &str,
        rule: &app_authz::Grant,
        at: DateTime<Utc>,
    ) -> AppResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO access.applied_default (module, role, resource, action, scope, applied_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            module,
            rule.role,
            rule.resource,
            rule.action,
            scope_name(&rule.scope),
            at,
        )
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }

    async fn gates_retired(&self, tx: &mut Tx) -> AppResult<bool> {
        sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM access.gates_retired) AS "retired!""#)
            .fetch_one(conn(tx))
            .await
            .map_err(AppError::internal)
    }

    async fn mark_gates_retired(&self, tx: &mut Tx, at: DateTime<Utc>) -> AppResult<()> {
        sqlx::query!(
            r#"INSERT INTO access.gates_retired (retired_at) VALUES ($1)"#,
            at
        )
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }
}
