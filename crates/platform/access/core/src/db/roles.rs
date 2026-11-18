use crate::application::{ListQuery, Listed, RoleRepository, RoleSortKey, Tx, Version, Versioned};
use crate::domain::{GrantedBy, Role};
use app_authz::Rows;
use app_db::{Clause, Column, Columns, conn, map_db_error, push_list_clauses, push_scope};
use app_error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, QueryBuilder};

pub struct PgRoleRepository {
    pool: PgPool,
}

impl PgRoleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_sqlx(err: sqlx::Error) -> AppError {
    map_db_error(err, |constraint| match constraint {
        "role_pkey" => Some(AppError::conflict("a role of that name exists already")),
        "grant_role_exists" => Some(AppError::conflict(
            "a grant still names that role: revoke it first",
        )),
        "assignment_role_exists" => Some(AppError::conflict(
            "the role is assigned to a subject still: unassign it first",
        )),
        _ => None,
    })
}

/// Where a role's attributes are, for a plan's filter to become a list's `WHERE` (ADR 0040).
pub struct RoleColumns;

impl Columns for RoleColumns {
    type Resource = Role;

    fn column(_: &str) -> Option<Column> {
        None
    }
}

fn sort_column(key: RoleSortKey) -> &'static str {
    match key {
        RoleSortKey::Name => "name",
        RoleSortKey::CreatedAt => "created_at",
    }
}

#[derive(sqlx::FromRow)]
struct RoleRow {
    name: String,
    description: String,
    created_by: Option<String>,
    created_at: DateTime<Utc>,
    version: i64,
}

impl RoleRow {
    fn restore(self) -> Versioned<Role> {
        Versioned {
            entity: Role::restore(
                self.name,
                self.description,
                self.created_by.map_or(GrantedBy::System, GrantedBy::User),
                self.created_at,
            ),
            version: Version::from_i64(self.version),
        }
    }
}

#[async_trait::async_trait]
impl RoleRepository for PgRoleRepository {
    async fn get(&self, name: &str) -> AppResult<Option<Versioned<Role>>> {
        let found = sqlx::query_as!(
            RoleRow,
            r#"SELECT name, description, created_by, created_at, version
               FROM access.role WHERE name = $1"#,
            name,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;
        Ok(found.map(RoleRow::restore))
    }

    async fn list(
        &self,
        query: &ListQuery<RoleSortKey>,
        rows: &Rows,
    ) -> AppResult<Listed<Versioned<Role>>> {
        let mut sql = QueryBuilder::new(
            "SELECT name, description, created_by, created_at, version FROM access.role",
        );
        push_scope::<RoleColumns>(&mut sql, Clause::Where, rows);
        push_list_clauses(&mut sql, query, sort_column, "name");
        let found: Vec<RoleRow> = sql
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;

        let total = match query.page {
            Some(_) => {
                let mut count = QueryBuilder::new("SELECT COUNT(*) FROM access.role");
                push_scope::<RoleColumns>(&mut count, Clause::Where, rows);
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
            items: found.into_iter().map(RoleRow::restore).collect(),
            total,
        })
    }

    async fn insert(&self, tx: &mut Tx, role: &Role) -> AppResult<Version> {
        let version = sqlx::query_scalar!(
            r#"
            INSERT INTO access.role (name, description, created_by, created_at, version)
            VALUES ($1, $2, $3, $4,
                    COALESCE((SELECT version FROM access.deleted_role WHERE name = $1), 0) + 1)
            RETURNING version
            "#,
            role.name(),
            role.description(),
            role.created_by().subject(),
            role.created_at(),
        )
        .fetch_one(conn(tx))
        .await
        .map_err(map_sqlx)?;
        Ok(Version::from_i64(version))
    }

    async fn exists(&self, tx: &mut Tx, name: &str) -> AppResult<bool> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM access.role WHERE name = $1) AS "found!""#,
            name,
        )
        .fetch_one(conn(tx))
        .await
        .map_err(AppError::internal)
    }

    async fn delete(&self, tx: &mut Tx, name: &str, expected: Version) -> AppResult<()> {
        let deleted = sqlx::query!(
            r#"DELETE FROM access.role WHERE name = $1 AND version = $2"#,
            name,
            expected.as_i64(),
        )
        .execute(conn(tx))
        .await
        .map_err(map_sqlx)?;
        if deleted.rows_affected() == 0 {
            return Err(AppError::precondition_failed(
                "the role was deleted meanwhile",
            ));
        }
        sqlx::query!(
            r#"
            INSERT INTO access.deleted_role (name, version) VALUES ($1, $2)
            ON CONFLICT (name) DO UPDATE SET version = EXCLUDED.version
            "#,
            name,
            expected.next().as_i64(),
        )
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }
}
