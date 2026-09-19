//! The persistence interface owned by the application layer.

use super::{Expected, Version, Versioned};
use crate::domain::{Email, User, UserId};
use app_error::AppResult;

/// Loads and saves `User` aggregates. The db layer implements it over Postgres.
///
/// It speaks in aggregates, never rows. A single-item `get` returns the user
/// paired with its stored [`Version`], so a subsequent [`save`](Self::save) can
/// prove which state it read and be refused if the row has since changed
/// (optimistic concurrency). `find_by_email` is an existence check feeding the
/// uniqueness rule, not a load-for-edit, so it carries no version.
#[async_trait::async_trait]
pub trait UserRepository: Send + Sync {
    async fn get(&self, id: UserId) -> AppResult<Option<Versioned<User>>>;
    async fn find_by_email(&self, email: &Email) -> AppResult<Option<User>>;
    /// Returns a page of users, most-recent first, plus the total count.
    async fn list(&self, limit: i64, offset: i64) -> AppResult<(Vec<Versioned<User>>, i64)>;
    /// Persists a user. `expected` says what must already be stored: [`Expected::New`]
    /// inserts; [`Expected::Version`] updates only if the stored version matches,
    /// returning a `PreconditionFailed` error otherwise. Returns the new version.
    async fn save(&self, user: &User, expected: Expected) -> AppResult<Version>;
}
