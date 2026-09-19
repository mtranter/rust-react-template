//! `UserService`: the users use cases.

use super::{Expected, UserRepository, Version, Versioned};
use crate::domain::{ChangeDisplayName, RegisterUser, User, UserId};
use app_error::{AppError, AppResult};
use chrono::{DateTime, SubsecRound, Utc};
use std::sync::Arc;

/// Coordinates the users use cases over a [`UserRepository`]. Supplies what the
/// domain must not produce itself: fresh identities and the current time.
///
/// Holds the repository as a trait object rather than being generic over it, so
/// the service is one concrete type: it can be named, stored, and shared without
/// threading a type parameter through every caller. The cost is one dynamic call
/// per repository method, negligible beside the I/O behind it.
#[derive(Clone)]
pub struct UserService {
    repo: Arc<dyn UserRepository>,
}

impl UserService {
    pub fn new(repo: Arc<dyn UserRepository>) -> Self {
        Self { repo }
    }

    /// Register a user, enforcing email uniqueness across all users.
    pub async fn register(&self, cmd: RegisterUser) -> AppResult<Versioned<User>> {
        let user = User::register(UserId::new(), cmd, now())?;
        if self.repo.find_by_email(user.email()).await?.is_some() {
            return Err(AppError::conflict("a user with that email already exists"));
        }
        let version = self.repo.save(&user, Expected::New).await?;
        Ok(Versioned {
            entity: user,
            version,
        })
    }

    /// Change a user's display name: load, act, save. `expected` is the version
    /// the caller last read (from `If-Match`); the save is refused if the account
    /// has since changed — e.g. the same user renamed themselves in another tab.
    pub async fn change_display_name(
        &self,
        cmd: ChangeDisplayName,
        expected: Version,
    ) -> AppResult<Versioned<User>> {
        let mut user = self.get(cmd.user_id).await?.entity;
        user.change_display_name(cmd)?;
        let version = self.repo.save(&user, Expected::Version(expected)).await?;
        Ok(Versioned {
            entity: user,
            version,
        })
    }

    pub async fn get(&self, id: UserId) -> AppResult<Versioned<User>> {
        self.repo
            .get(id)
            .await?
            .ok_or_else(|| AppError::not_found("user"))
    }

    /// List users with clamped pagination bounds applied by the caller.
    pub async fn list(&self, limit: i64, offset: i64) -> AppResult<(Vec<Versioned<User>>, i64)> {
        self.repo.list(limit, offset).await
    }
}

/// The current time at Postgres' `timestamptz` precision (microseconds), so an
/// aggregate returned straight after `save` matches the same one read back.
fn now() -> DateTime<Utc> {
    Utc::now().trunc_subsecs(6)
}
