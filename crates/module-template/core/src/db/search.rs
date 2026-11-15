use crate::application::{Listed, PageRequest, Tx, WidgetMatch, WidgetSearch};
use app_db::{Generations, Project, conn};
use app_error::{AppError, AppResult};
use app_events::TopicEnvelope;
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;
use widgets_api::events::v1::{WidgetEvent, WidgetTopic};

/// The queue feeding the first generation of the copy: renaming it strands
/// the copy, so a new shape is a new generation (a `_v2` queue and table).
pub const SEARCH: &str = "widgets.search";

pub struct SearchRows;

#[async_trait]
impl Project<WidgetTopic> for SearchRows {
    fn removes(&self, _event: &WidgetEvent) -> bool {
        false
    }

    async fn put(&self, tx: &mut Tx, event: &TopicEnvelope<WidgetTopic>) -> AppResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO widgets.widget_search (id, name) VALUES ($1, $2)
            ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name
            "#,
            event.entity.id,
            event.entity.name,
        )
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }

    async fn remove(&self, tx: &mut Tx, key: &str) -> AppResult<()> {
        let id: Uuid = key.parse().map_err(AppError::internal)?;
        sqlx::query!("DELETE FROM widgets.widget_search WHERE id = $1", id)
            .execute(conn(tx))
            .await
            .map_err(AppError::internal)?;
        Ok(())
    }
}

/// Searches the copy, from the newest generation of its table that has caught
/// up (ADR 0036).
pub struct PgWidgetSearch {
    pool: PgPool,
    generations: Generations<&'static str>,
}

impl PgWidgetSearch {
    pub fn new(pool: PgPool) -> Self {
        // A rebuilt copy goes first, with its own queue, until the old one retires.
        let generations = Generations::new(pool.clone()).then("widgets.widget_search", &[SEARCH]);
        Self { pool, generations }
    }
}

#[derive(sqlx::FromRow)]
struct MatchRow {
    id: Uuid,
    name: String,
}

/// Backslash is `LIKE`'s default escape character.
fn starts_with(name: &str) -> String {
    let escaped = name
        .to_lowercase()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("{escaped}%")
}

/// `lower(name) LIKE` matches the index's `text_pattern_ops`.
fn push_matching(sql: &mut QueryBuilder<Postgres>, table: &str, name: &str) {
    sql.push(format_args!(" FROM {table} WHERE lower(name) LIKE "))
        .push_bind(starts_with(name));
}

#[async_trait]
impl WidgetSearch for PgWidgetSearch {
    async fn search(&self, name: &str, page: PageRequest) -> AppResult<Listed<WidgetMatch>> {
        let table = self.generations.serving().await?;
        let mut sql = QueryBuilder::new("SELECT id, name");
        push_matching(&mut sql, table, name);
        sql.push(" ORDER BY lower(name), id LIMIT ")
            .push_bind(page.limit)
            .push(" OFFSET ")
            .push_bind(page.offset);
        let rows: Vec<MatchRow> = sql
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;

        let mut count = QueryBuilder::new("SELECT COUNT(*)");
        push_matching(&mut count, table, name);
        let total: i64 = count
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await
            .map_err(AppError::internal)?;

        Ok(Listed {
            items: rows
                .into_iter()
                .map(|r| WidgetMatch {
                    id: r.id,
                    name: r.name,
                })
                .collect(),
            total: Some(total),
        })
    }
}
