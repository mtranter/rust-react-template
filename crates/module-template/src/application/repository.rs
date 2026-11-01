use super::{Expected, ListQuery, Listed, Version, Versioned};
use crate::domain::{Widget, WidgetId};
use app_error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetSortKey {
    Name,
    CreatedAt,
}

#[async_trait::async_trait]
pub trait WidgetRepository: Send + Sync {
    async fn get(&self, id: WidgetId) -> AppResult<Option<Versioned<Widget>>>;
    /// Most-recent first unless sorted otherwise.
    async fn list(&self, query: &ListQuery<WidgetSortKey>) -> AppResult<Listed<Versioned<Widget>>>;
    /// [`Expected::Version`] updates only if the stored version matches, else
    /// `PreconditionFailed`. Returns the new version.
    async fn save(&self, widget: &Widget, expected: Expected) -> AppResult<Version>;
}
