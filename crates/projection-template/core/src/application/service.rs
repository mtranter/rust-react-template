use super::{Entry, EntryFinder, EntrySortKey, ListQuery, Listed};
use app_error::AppResult;
use std::sync::Arc;

#[derive(Clone)]
pub struct WidgetIndexService {
    finder: Arc<dyn EntryFinder>,
}

impl WidgetIndexService {
    pub fn new(finder: Arc<dyn EntryFinder>) -> Self {
        Self { finder }
    }

    pub async fn list(&self, query: ListQuery<EntrySortKey>) -> AppResult<Listed<Entry>> {
        self.finder.list(&query).await
    }
}
