use crate::application::{
    Expected, ListQuery, Listed, Version, Versioned, WidgetRepository, WidgetSortKey,
};
use crate::domain::{Widget, WidgetId};
use app_db::push_list_clauses;
use app_error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, QueryBuilder};
use uuid::Uuid;

pub struct PgWidgetRepository {
    pool: PgPool,
}

impl PgWidgetRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn sort_column(key: WidgetSortKey) -> &'static str {
    match key {
        WidgetSortKey::Name => "name",
        WidgetSortKey::CreatedAt => "created_at",
    }
}

/// Lists build their SQL at runtime (the sort varies), so they can't use `query!`.
#[derive(sqlx::FromRow)]
struct WidgetRow {
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
    version: i64,
}

impl WidgetRow {
    fn restore(r: Self) -> Versioned<Widget> {
        Versioned {
            entity: Widget::restore(r.id.into(), r.name, r.created_at),
            version: Version::from_i64(r.version),
        }
    }
}

#[async_trait::async_trait]
impl WidgetRepository for PgWidgetRepository {
    async fn get(&self, id: WidgetId) -> AppResult<Option<Versioned<Widget>>> {
        let rec = sqlx::query!(
            r#"SELECT id, name, created_at, version FROM widgets.widget WHERE id = $1"#,
            id.as_uuid(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;

        Ok(rec.map(|r| Versioned {
            entity: Widget::restore(r.id.into(), r.name, r.created_at),
            version: Version::from_i64(r.version),
        }))
    }

    async fn list(&self, query: &ListQuery<WidgetSortKey>) -> AppResult<Listed<Versioned<Widget>>> {
        let mut sql = QueryBuilder::new("SELECT id, name, created_at, version FROM widgets.widget");
        push_list_clauses(&mut sql, query, sort_column, "created_at DESC, id");
        let rows: Vec<WidgetRow> = sql
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;

        let total = match query.page {
            Some(_) => sqlx::query_scalar!(r#"SELECT COUNT(*) FROM widgets.widget"#)
                .fetch_one(&self.pool)
                .await
                .map_err(AppError::internal)?,
            None => None,
        };

        Ok(Listed {
            items: rows.into_iter().map(WidgetRow::restore).collect(),
            total,
        })
    }

    async fn save(&self, widget: &Widget, expected: Expected) -> AppResult<Version> {
        match expected {
            // A duplicate id is an internal error: ids are minted, not client-chosen.
            Expected::New => {
                let version = sqlx::query_scalar!(
                    r#"
                    INSERT INTO widgets.widget (id, name, created_at, version)
                    VALUES ($1, $2, $3, 1)
                    RETURNING version
                    "#,
                    widget.id().as_uuid(),
                    widget.name(),
                    widget.created_at(),
                )
                .fetch_one(&self.pool)
                .await
                .map_err(AppError::internal)?;
                Ok(Version::from_i64(version))
            }
            // No row updated ⇒ the widget changed since the caller read it.
            Expected::Version(expected) => {
                let updated = sqlx::query_scalar!(
                    r#"
                    UPDATE widgets.widget
                    SET name = $2, version = version + 1
                    WHERE id = $1 AND version = $3
                    RETURNING version
                    "#,
                    widget.id().as_uuid(),
                    widget.name(),
                    expected.as_i64(),
                )
                .fetch_optional(&self.pool)
                .await
                .map_err(AppError::internal)?;

                updated.map(Version::from_i64).ok_or_else(|| {
                    AppError::precondition_failed("the widget was modified by someone else")
                })
            }
        }
    }
}
