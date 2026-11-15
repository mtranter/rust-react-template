use super::{Listed, PageRequest};
use app_error::AppResult;
use uuid::Uuid;

/// A read model from the module's search copy, not the aggregate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetMatch {
    pub id: Uuid,
    pub name: String,
}

#[async_trait::async_trait]
pub trait WidgetSearch: Send + Sync {
    /// By name, case-insensitively by its start; `unavailable` until the copy
    /// has caught up.
    async fn search(&self, name: &str, page: PageRequest) -> AppResult<Listed<WidgetMatch>>;
}
