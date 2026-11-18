use super::NoteSummaryAction;
use super::{ListQuery, Listed, NoteCriteria, NoteFinder, NoteSortKey, NoteSummary};
use app_authz::{Authorizer, Plan, Principal};
use app_error::{AppError, AppResult};
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

    /// The copy has no columns for a scope to filter by, so a scoped search
    /// is refused rather than shown rows the caller may not see.
    pub async fn search(
        &self,
        principal: &Principal,
        text: Option<String>,
        query: ListQuery<NoteSortKey>,
    ) -> AppResult<Listed<NoteSummary>> {
        if self
            .authorizer
            .authorize::<NoteSummary>(principal, NoteSummaryAction::Search)?
            != Plan::Allowed
        {
            return Err(AppError::Forbidden);
        }
        self.finder
            .find(&NoteCriteria::matching(text), &query)
            .await
    }
}
