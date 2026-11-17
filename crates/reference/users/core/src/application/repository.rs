use super::{Expected, ListQuery, Listed, Tx, Version, Versioned};
use crate::domain::{Email, User, UserId};
use app_error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserSortKey {
    Email,
    DisplayName,
    CreatedAt,
}

/// Reads run on the pool; writes run in the caller's unit of work.
/// `find_by_email` feeds the uniqueness rule, not a load-for-edit, so unlike
/// `get` it carries no version.
#[async_trait::async_trait]
pub trait UserRepository: Send + Sync {
    async fn get(&self, id: UserId) -> AppResult<Option<Versioned<User>>>;
    async fn find_by_email(&self, email: &Email) -> AppResult<Option<User>>;
    /// Most-recent first unless sorted otherwise.
    async fn list(&self, query: &ListQuery<UserSortKey>) -> AppResult<Listed<Versioned<User>>>;
    /// A stale [`Expected::Version`] is refused with `PreconditionFailed`.
    async fn save(&self, tx: &mut Tx, user: &User, expected: Expected) -> AppResult<Version>;
    /// A stale `expected` is refused with `PreconditionFailed`.
    async fn delete(&self, tx: &mut Tx, id: UserId, expected: Version) -> AppResult<()>;
}
