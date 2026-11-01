use super::{Expected, ListQuery, Listed, Version, Versioned, WidgetRepository, WidgetSortKey};
use crate::domain::{CreateWidget, RenameWidget, Widget, WidgetId};
use app_error::{AppError, AppResult};
use std::sync::Arc;

#[derive(Clone)]
pub struct WidgetService {
    repo: Arc<dyn WidgetRepository>,
}

impl WidgetService {
    pub fn new(repo: Arc<dyn WidgetRepository>) -> Self {
        Self { repo }
    }

    pub async fn create(&self, cmd: CreateWidget) -> AppResult<Versioned<Widget>> {
        let widget = Widget::create(WidgetId::new(), cmd, app_domain::now())?;
        let version = self.repo.save(&widget, Expected::New).await?;
        Ok(Versioned {
            entity: widget,
            version,
        })
    }

    /// `expected` is the version the caller last read (from `If-Match`).
    pub async fn rename(
        &self,
        cmd: RenameWidget,
        expected: Version,
    ) -> AppResult<Versioned<Widget>> {
        // Load first so a missing widget is a not-found rather than a stale-version
        // conflict; the version check itself is atomic in the repository's update.
        let mut widget = self.get(cmd.widget_id).await?.entity;
        widget.rename(cmd)?;
        let version = self.repo.save(&widget, Expected::Version(expected)).await?;
        Ok(Versioned {
            entity: widget,
            version,
        })
    }

    pub async fn get(&self, id: WidgetId) -> AppResult<Versioned<Widget>> {
        self.repo
            .get(id)
            .await?
            .ok_or_else(|| AppError::not_found("widget"))
    }

    pub async fn list(
        &self,
        query: ListQuery<WidgetSortKey>,
    ) -> AppResult<Listed<Versioned<Widget>>> {
        self.repo.list(&query).await
    }
}
