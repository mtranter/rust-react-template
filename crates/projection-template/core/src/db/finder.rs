use super::COPY;
use crate::application::{Entry, EntryFinder, EntrySortKey, ListQuery, Listed};
use app_db::{Generations, push_list_clauses};
use app_error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, QueryBuilder};

/// Reads this module's own copy, from the newest generation of its table that
/// has caught up (ADR 0036).
pub struct PgEntryFinder {
    pool: PgPool,
    generations: Generations<&'static str>,
}

impl PgEntryFinder {
    pub fn new(pool: PgPool) -> Self {
        // A rebuilt copy goes first, with its own queue, until the old one retires.
        let generations = Generations::new(pool.clone()).then("widget_index.entry", &[COPY]);
        Self { pool, generations }
    }
}

fn sort_column(key: EntrySortKey) -> &'static str {
    match key {
        EntrySortKey::UpdatedAt => "updated_at",
    }
}

#[derive(sqlx::FromRow)]
struct EntryRow {
    key: String,
    updated_at: DateTime<Utc>,
}

#[async_trait::async_trait]
impl EntryFinder for PgEntryFinder {
    async fn list(&self, query: &ListQuery<EntrySortKey>) -> AppResult<Listed<Entry>> {
        let table = self.generations.serving().await?;
        let mut sql = QueryBuilder::new("SELECT key, updated_at FROM ");
        sql.push(table);
        push_list_clauses(&mut sql, query, sort_column, "updated_at DESC, key");
        let rows: Vec<EntryRow> = sql
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;

        let total = match query.page {
            Some(_) => {
                let mut count = QueryBuilder::new("SELECT COUNT(*) FROM ");
                count.push(table);
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
            items: rows
                .into_iter()
                .map(|r| Entry {
                    key: r.key,
                    updated_at: r.updated_at,
                })
                .collect(),
            total,
        })
    }
}
