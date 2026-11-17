use super::{ListQuery, Listed, NoteSummary};
use app_error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteSortKey {
    Title,
    AuthorName,
    CreatedAt,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteCriteria {
    /// Matched case-insensitively, as literal text, anywhere in a note's title
    /// or its author's name.
    pub text: Option<String>,
}

impl NoteCriteria {
    /// Blank text matches every note.
    pub fn matching(text: Option<String>) -> Self {
        let text = text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        Self { text }
    }
}

/// The read-side counterpart of a repository: it answers queries with read
/// models and never loads or saves aggregates.
#[async_trait::async_trait]
pub trait NoteFinder: Send + Sync {
    /// Most-recent first unless sorted otherwise.
    async fn find(
        &self,
        criteria: &NoteCriteria,
        query: &ListQuery<NoteSortKey>,
    ) -> AppResult<Listed<NoteSummary>>;
}
