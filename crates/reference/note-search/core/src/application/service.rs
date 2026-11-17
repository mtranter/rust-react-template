use super::{ListQuery, Listed, NoteCriteria, NoteFinder, NoteSortKey, NoteSummary};
use app_error::AppResult;
use std::sync::Arc;

#[derive(Clone)]
pub struct NoteSearchService {
    finder: Arc<dyn NoteFinder>,
}

impl NoteSearchService {
    pub fn new(finder: Arc<dyn NoteFinder>) -> Self {
        Self { finder }
    }

    pub async fn search(
        &self,
        text: Option<String>,
        query: ListQuery<NoteSortKey>,
    ) -> AppResult<Listed<NoteSummary>> {
        self.finder
            .find(&NoteCriteria::matching(text), &query)
            .await
    }
}
