use crate::application::{ListQuery, Listed, SubjectFilter, SubjectRepository, SubjectSortKey};
use crate::domain::{Sighting, Subject};
use app_authz::Rows;
use app_db::{Clause, Column, Columns, push_list_clauses, push_scope};
use app_error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};

pub struct PgSubjectRepository {
    pool: PgPool,
}

impl PgSubjectRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Where a subject's attributes are, for a plan's filter to become a list's `WHERE` (ADR 0040).
pub struct SubjectColumns;

impl Columns for SubjectColumns {
    type Resource = Subject;

    fn column(_: &str) -> Option<Column> {
        None
    }
}

fn sort_column(key: SubjectSortKey) -> &'static str {
    match key {
        SubjectSortKey::Subject => "subject",
        SubjectSortKey::Name => "name",
        SubjectSortKey::Email => "email",
        SubjectSortKey::LastSeen => "last_seen",
    }
}

#[derive(sqlx::FromRow)]
struct SubjectRow {
    subject: String,
    name: Option<String>,
    email: Option<String>,
    first_seen: Option<DateTime<Utc>>,
    last_seen: Option<DateTime<Utc>>,
}

impl From<SubjectRow> for Subject {
    fn from(row: SubjectRow) -> Self {
        Self {
            subject: row.subject,
            name: row.name,
            email: row.email,
            first_seen: row.first_seen,
            last_seen: row.last_seen,
        }
    }
}

/// Subjects seen, assigned a role or bootstrap admins, as one relation to filter, sort and page.
fn push_known(
    sql: &mut QueryBuilder<Postgres>,
    filter: &SubjectFilter,
    bootstrap_admins: &[String],
    rows: &Rows,
) {
    sql.push(
        " FROM (SELECT k.subject, s.name, s.email, s.first_seen, s.last_seen \
           FROM (SELECT subject FROM access.subject \
                 UNION SELECT subject FROM access.assignment \
                 UNION SELECT unnest(",
    )
    .push_bind(bootstrap_admins)
    .push("::text[])) AS k LEFT JOIN access.subject s ON s.subject = k.subject) AS known")
    .push(" WHERE TRUE");
    push_scope::<SubjectColumns>(sql, Clause::And, rows);
    if let Some(search) = filter.search.as_deref().map(str::trim)
        && !search.is_empty()
    {
        let pattern = format!("%{}%", escape_like(search));
        sql.push(" AND (subject ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR email ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
}

fn escape_like(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[async_trait::async_trait]
impl SubjectRepository for PgSubjectRepository {
    async fn record(&self, sighting: &Sighting, stale_before: DateTime<Utc>) -> AppResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO access.subject AS s (subject, name, email, first_seen, last_seen)
            VALUES ($1, $2, $3, $4, $4)
            ON CONFLICT (subject) DO UPDATE SET
                name = COALESCE(EXCLUDED.name, s.name),
                email = COALESCE(EXCLUDED.email, s.email),
                last_seen = GREATEST(s.last_seen, EXCLUDED.last_seen)
            WHERE s.last_seen <= $5
               OR (EXCLUDED.name IS NOT NULL AND EXCLUDED.name IS DISTINCT FROM s.name)
               OR (EXCLUDED.email IS NOT NULL AND EXCLUDED.email IS DISTINCT FROM s.email)
            "#,
            sighting.subject,
            sighting.name,
            sighting.email,
            sighting.at,
            stale_before,
        )
        .execute(&self.pool)
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }

    async fn list(
        &self,
        filter: &SubjectFilter,
        bootstrap_admins: &[String],
        query: &ListQuery<SubjectSortKey>,
        rows: &Rows,
    ) -> AppResult<Listed<Subject>> {
        let mut sql = QueryBuilder::new("SELECT subject, name, email, first_seen, last_seen");
        push_known(&mut sql, filter, bootstrap_admins, rows);
        push_list_clauses(&mut sql, query, sort_column, "subject");
        let found: Vec<SubjectRow> = sql
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;

        let total = match query.page {
            Some(_) => {
                let mut count = QueryBuilder::new("SELECT COUNT(*)");
                push_known(&mut count, filter, bootstrap_admins, rows);
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
            items: found.into_iter().map(Subject::from).collect(),
            total,
        })
    }
}
