use crate::application::{
    InvitationFilter, InvitationRepository, InvitationSortKey, ListQuery, Listed, Tx, Version,
    Versioned,
};
use crate::domain::{Email, GrantedBy, Invitation, InvitationId, InvitationStatus};
use app_authz::Rows;
use app_db::{Clause, Column, Columns, conn, map_db_error, push_list_clauses, push_scope};
use app_error::{AppError, AppResult};
use app_persistence::Expected;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

pub struct PgInvitationRepository {
    pool: PgPool,
}

impl PgInvitationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_sqlx(err: sqlx::Error) -> AppError {
    map_db_error(err, |constraint| match constraint {
        "invitation_pending_email" => Some(AppError::conflict(
            "an invitation to that email is pending already: resend it instead",
        )),
        _ => None,
    })
}

/// Where an invitation's attributes are, for a plan's filter to become a list's `WHERE` (ADR 0040).
pub struct InvitationColumns;

impl Columns for InvitationColumns {
    type Resource = Invitation;

    fn column(_: &str) -> Option<Column> {
        None
    }
}

fn sort_column(key: InvitationSortKey) -> &'static str {
    match key {
        InvitationSortKey::Email => "email",
        InvitationSortKey::InvitedAt => "invited_at",
    }
}

#[derive(sqlx::FromRow)]
struct InvitationRow {
    id: Uuid,
    email: String,
    name: String,
    subject: String,
    roles: Vec<String>,
    invited_by: Option<String>,
    invited_at: DateTime<Utc>,
    status: String,
    version: i64,
}

impl InvitationRow {
    fn restore(self) -> AppResult<Versioned<Invitation>> {
        let status: InvitationStatus = self.status.parse().map_err(AppError::internal)?;
        Ok(Versioned {
            entity: Invitation::restore(
                InvitationId::from_uuid(self.id),
                Email::restore(self.email),
                self.name,
                self.subject,
                self.roles,
                self.invited_by.map_or(GrantedBy::System, GrantedBy::User),
                self.invited_at,
                status,
            ),
            version: Version::from_i64(self.version),
        })
    }
}

const COLUMNS: &str =
    "SELECT id, email, name, subject, roles, invited_by, invited_at, status, version";

fn push_filtered(sql: &mut QueryBuilder<Postgres>, filter: &InvitationFilter, rows: &Rows) {
    sql.push(" FROM access.invitation WHERE TRUE");
    push_scope::<InvitationColumns>(sql, Clause::And, rows);
    if let Some(status) = filter.status {
        let status: &'static str = status.into();
        sql.push(" AND status = ").push_bind(status);
    }
}

fn restore_one(found: Option<InvitationRow>) -> AppResult<Option<Versioned<Invitation>>> {
    found.map(InvitationRow::restore).transpose()
}

#[async_trait::async_trait]
impl InvitationRepository for PgInvitationRepository {
    async fn get(&self, id: InvitationId) -> AppResult<Option<Versioned<Invitation>>> {
        let found = sqlx::query_as!(
            InvitationRow,
            r#"SELECT id, email, name, subject, roles, invited_by, invited_at, status, version
               FROM access.invitation WHERE id = $1"#,
            id.as_uuid(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;
        restore_one(found)
    }

    async fn pending_for(&self, email: &Email) -> AppResult<Option<Versioned<Invitation>>> {
        let found = sqlx::query_as!(
            InvitationRow,
            r#"SELECT id, email, name, subject, roles, invited_by, invited_at, status, version
               FROM access.invitation WHERE email = $1 AND status = 'pending'"#,
            email.as_str(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;
        restore_one(found)
    }

    async fn pending_of(
        &self,
        tx: &mut Tx,
        subject: &str,
    ) -> AppResult<Option<Versioned<Invitation>>> {
        let found = sqlx::query_as!(
            InvitationRow,
            r#"SELECT id, email, name, subject, roles, invited_by, invited_at, status, version
               FROM access.invitation WHERE subject = $1 AND status = 'pending'
               ORDER BY invited_at DESC LIMIT 1"#,
            subject,
        )
        .fetch_optional(conn(tx))
        .await
        .map_err(AppError::internal)?;
        restore_one(found)
    }

    async fn list(
        &self,
        filter: &InvitationFilter,
        query: &ListQuery<InvitationSortKey>,
        rows: &Rows,
    ) -> AppResult<Listed<Versioned<Invitation>>> {
        let mut sql = QueryBuilder::new(COLUMNS);
        push_filtered(&mut sql, filter, rows);
        push_list_clauses(&mut sql, query, sort_column, "invited_at DESC, id");
        let found: Vec<InvitationRow> = sql
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;

        let total = match query.page {
            Some(_) => {
                let mut count = QueryBuilder::new("SELECT COUNT(*)");
                push_filtered(&mut count, filter, rows);
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
            items: found
                .into_iter()
                .map(InvitationRow::restore)
                .collect::<AppResult<_>>()?,
            total,
        })
    }

    async fn save(
        &self,
        tx: &mut Tx,
        invitation: &Invitation,
        expected: Expected,
    ) -> AppResult<Version> {
        let status: &'static str = invitation.status().into();
        match expected {
            // A duplicate id is an internal error: ids are minted, not client-chosen.
            Expected::New => {
                let version = sqlx::query_scalar!(
                    r#"
                    INSERT INTO access.invitation
                        (id, email, name, subject, roles, invited_by, invited_at, status, version)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 1)
                    RETURNING version
                    "#,
                    invitation.id().as_uuid(),
                    invitation.email().as_str(),
                    invitation.name(),
                    invitation.subject(),
                    invitation.roles(),
                    invitation.invited_by().subject(),
                    invitation.invited_at(),
                    status,
                )
                .fetch_one(conn(tx))
                .await
                .map_err(map_sqlx)?;
                Ok(Version::from_i64(version))
            }
            Expected::Version(expected) => {
                let updated = sqlx::query_scalar!(
                    r#"
                    UPDATE access.invitation SET status = $2, version = version + 1
                    WHERE id = $1 AND version = $3
                    RETURNING version
                    "#,
                    invitation.id().as_uuid(),
                    status,
                    expected.as_i64(),
                )
                .fetch_optional(conn(tx))
                .await
                .map_err(map_sqlx)?;
                updated.map(Version::from_i64).ok_or_else(|| {
                    AppError::precondition_failed("the invitation changed meanwhile")
                })
            }
        }
    }
}
