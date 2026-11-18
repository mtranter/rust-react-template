use super::NoteSummaryAction;
use super::{ListQuery, Listed, NoteCriteria, NoteFinder, NoteSortKey, NoteSummary};
use app_authz::{Authorizer, Principal};
use app_error::AppResult;
use std::sync::Arc;

#[derive(Clone)]
pub struct NoteSearchService {
    finder: Arc<dyn NoteFinder>,
    authorizer: Arc<Authorizer>,
}

impl NoteSearchService {
    pub fn new(finder: Arc<dyn NoteFinder>, authorizer: Arc<Authorizer>) -> Self {
        Self { finder, authorizer }
    }

    /// A `Where` plan is allowed here until lists filter by it (platform
    /// ticket 0028).
    pub async fn search(
        &self,
        principal: &Principal,
        text: Option<String>,
        query: ListQuery<NoteSortKey>,
    ) -> AppResult<Listed<NoteSummary>> {
        self.authorizer
            .authorize::<NoteSummary>(principal, NoteSummaryAction::Search)?;
        self.finder
            .find(&NoteCriteria::matching(text), &query)
            .await
    }
}
