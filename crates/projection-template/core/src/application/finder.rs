use super::{Entry, ListQuery, Listed};
use app_error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntrySortKey {
    UpdatedAt,
}

/// The read-side counterpart of a repository: it answers queries with read
/// models and never loads or saves aggregates.
#[async_trait::async_trait]
pub trait EntryFinder: Send + Sync {
    /// Most recently updated first unless sorted otherwise; `unavailable`
    /// until the copy has caught up.
    async fn list(&self, query: &ListQuery<EntrySortKey>) -> AppResult<Listed<Entry>>;
}
