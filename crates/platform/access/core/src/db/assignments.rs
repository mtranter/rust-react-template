use crate::application::{AssignmentRepository, Tx, Version, Versioned};
use crate::domain::{Assignment, GrantedBy};
use app_db::{conn, map_db_error};
use app_error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub struct PgAssignmentRepository {
    pool: PgPool,
}

impl PgAssignmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_sqlx(err: sqlx::Error) -> AppError {
    map_db_error(err, |constraint| match constraint {
        "assignment_pkey" => Some(AppError::conflict(
            "that role is assigned to that subject already",
        )),
        "assignment_role_exists" => Some(AppError::invalid_field(
            "role",
            "no role has that name: create it first",
        )),
        _ => None,
    })
}

struct AssignmentRow {
    subject: String,
    role: String,
    assigned_by: Option<String>,
    assigned_at: DateTime<Utc>,
    version: i64,
}

impl AssignmentRow {
    fn restore(self) -> Versioned<Assignment> {
        Versioned {
            entity: Assignment::restore(
                self.subject,
                self.role,
                self.assigned_by.map_or(GrantedBy::System, GrantedBy::User),
                self.assigned_at,
            ),
            version: Version::from_i64(self.version),
        }
    }
}

#[async_trait::async_trait]
impl AssignmentRepository for PgAssignmentRepository {
    async fn get(&self, subject: &str, role: &str) -> AppResult<Option<Versioned<Assignment>>> {
        let found = sqlx::query_as!(
            AssignmentRow,
            r#"SELECT subject, role, assigned_by, assigned_at, version
               FROM access.assignment WHERE subject = $1 AND role = $2"#,
            subject,
            role,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;
        Ok(found.map(AssignmentRow::restore))
    }

    async fn all(&self) -> AppResult<Vec<(String, String)>> {
        let found = sqlx::query!(r#"SELECT subject, role FROM access.assignment"#)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;
        Ok(found
            .into_iter()
            .map(|row| (row.subject, row.role))
            .collect())
    }

    async fn insert(&self, tx: &mut Tx, assignment: &Assignment) -> AppResult<Version> {
        let version = sqlx::query_scalar!(
            r#"
            INSERT INTO access.assignment (subject, role, assigned_by, assigned_at, version)
            VALUES ($1, $2, $3, $4,
                    COALESCE((SELECT version FROM access.unassigned
                              WHERE subject = $1 AND role = $2), 0) + 1)
            RETURNING version
            "#,
            assignment.subject(),
            assignment.role(),
            assignment.assigned_by().subject(),
            assignment.assigned_at(),
        )
        .fetch_one(conn(tx))
        .await
        .map_err(map_sqlx)?;
        Ok(Version::from_i64(version))
    }

    async fn holders(&self, tx: &mut Tx, role: &str) -> AppResult<i64> {
        sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM access.assignment WHERE role = $1"#,
            role,
        )
        .fetch_one(conn(tx))
        .await
        .map_err(AppError::internal)
    }

    async fn delete(
        &self,
        tx: &mut Tx,
        subject: &str,
        role: &str,
        expected: Version,
    ) -> AppResult<()> {
        let deleted = sqlx::query!(
            r#"DELETE FROM access.assignment WHERE subject = $1 AND role = $2 AND version = $3"#,
            subject,
            role,
            expected.as_i64(),
        )
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        if deleted.rows_affected() == 0 {
            return Err(AppError::precondition_failed(
                "the role was unassigned meanwhile",
            ));
        }
        sqlx::query!(
            r#"
            INSERT INTO access.unassigned (subject, role, version) VALUES ($1, $2, $3)
            ON CONFLICT (subject, role) DO UPDATE SET version = EXCLUDED.version
            "#,
            subject,
            role,
            expected.next().as_i64(),
        )
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }
}
