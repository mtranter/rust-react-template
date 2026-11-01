use crate::application::{ListQuery, Listed, NoteCriteria, NoteFinder, NoteSortKey, NoteSummary};
use app_db::push_list_clauses;
use app_error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

/// Reads only the published views declared in `Cargo.toml`'s
/// `[package.metadata.module] reads` (spec D8).
pub struct PgNoteFinder {
    pool: PgPool,
}

impl PgNoteFinder {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn sort_column(key: NoteSortKey) -> &'static str {
    match key {
        NoteSortKey::Title => "n.title",
        NoteSortKey::AuthorName => "u.display_name",
        NoteSortKey::CreatedAt => "n.created_at",
    }
}

#[derive(sqlx::FromRow)]
struct SummaryRow {
    id: Uuid,
    title: String,
    author_id: Uuid,
    author_name: Option<String>,
    created_at: DateTime<Utc>,
}

impl From<SummaryRow> for NoteSummary {
    fn from(r: SummaryRow) -> Self {
        Self {
            id: r.id,
            title: r.title,
            author_id: r.author_id,
            author_name: r.author_name,
            created_at: r.created_at,
        }
    }
}

/// A left join: a note whose author id no user has is still a note.
fn push_matching(sql: &mut QueryBuilder<Postgres>, criteria: &NoteCriteria) {
    sql.push(" FROM notes_api.note_v1 n LEFT JOIN users_api.user_v1 u ON u.id = n.author_id");
    if let Some(text) = &criteria.text {
        let pattern = format!("%{}%", escape_like(text));
        sql.push(" WHERE n.title ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR u.display_name ILIKE ")
            .push_bind(pattern);
    }
}

/// Backslash is `LIKE`'s default escape character.
fn escape_like(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[async_trait::async_trait]
impl NoteFinder for PgNoteFinder {
    async fn find(
        &self,
        criteria: &NoteCriteria,
        query: &ListQuery<NoteSortKey>,
    ) -> AppResult<Listed<NoteSummary>> {
        let mut sql = QueryBuilder::new(
            "SELECT n.id, n.title, n.author_id, u.display_name AS author_name, n.created_at",
        );
        push_matching(&mut sql, criteria);
        push_list_clauses(&mut sql, query, sort_column, "n.created_at DESC, n.id");
        let rows: Vec<SummaryRow> = sql
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;

        let total = match query.page {
            Some(_) => {
                let mut count = QueryBuilder::new("SELECT COUNT(*)");
                push_matching(&mut count, criteria);
                let total: i64 = count
                    .build_query_scalar()
                    .fetch_one(&self.pool)
                    .await
                    .map_err(AppError::internal)?;
                Some(total)
            }
            None => None,
        };

        Ok(Listed {
            items: rows.into_iter().map(NoteSummary::from).collect(),
            total,
        })
    }
}
