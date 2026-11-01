use super::{Expected, ListQuery, Listed, Version, Versioned};
use crate::domain::{Email, User, UserId};
use app_error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserSortKey {
    Email,
    DisplayName,
    CreatedAt,
}

/// `find_by_email` feeds the uniqueness rule, not a load-for-edit, so unlike
/// `get` it carries no version.
#[async_trait::async_trait]
pub trait UserRepository: Send + Sync {
    async fn get(&self, id: UserId) -> AppResult<Option<Versioned<User>>>;
    async fn find_by_email(&self, email: &Email) -> AppResult<Option<User>>;
    /// Most-recent first unless sorted otherwise.
    async fn list(&self, query: &ListQuery<UserSortKey>) -> AppResult<Listed<Versioned<User>>>;
    /// A stale [`Expected::Version`] is refused with `PreconditionFailed`.
    async fn save(&self, user: &User, expected: Expected) -> AppResult<Version>;
}
