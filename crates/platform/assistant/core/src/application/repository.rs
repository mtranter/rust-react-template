use super::{Expected, ListQuery, Listed, Tx, Version, Versioned};
use crate::domain::{Conversation, ConversationId};
use app_authz::Rows;
use app_error::AppResult;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationSortKey {
    CreatedAt,
}

/// A conversation as a list shows it, without its messages, which a list has no use for and
/// which grow with every turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSummary {
    pub id: ConversationId,
    pub title: String,
    pub created_at: DateTime<Utc>,
}

/// Single-item loads pair the aggregate with its stored [`Version`] so a later
/// [`save`](Self::save) is refused if the row has changed since. Reads run on the pool; writes
/// run in the caller's unit of work.
#[async_trait::async_trait]
pub trait ConversationRepository: Send + Sync {
    async fn get(&self, id: ConversationId) -> AppResult<Option<Versioned<Conversation>>>;
    /// Only the rows the caller may see, counted in the total too. Newest first unless sorted
    /// otherwise.
    async fn list(
        &self,
        query: &ListQuery<ConversationSortKey>,
        rows: &Rows,
    ) -> AppResult<Listed<Versioned<ConversationSummary>>>;
    /// [`Expected::Version`] updates only if the stored version matches, else
    /// `PreconditionFailed`. Returns the new version.
    async fn save(
        &self,
        tx: &mut Tx,
        conversation: &Conversation,
        expected: Expected,
    ) -> AppResult<Version>;
    /// Only if the stored version is `expected`, else `PreconditionFailed`.
    async fn delete(&self, tx: &mut Tx, id: ConversationId, expected: Version) -> AppResult<()>;
}
