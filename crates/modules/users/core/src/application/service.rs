use super::{Expected, ListQuery, Listed, UserRepository, UserSortKey, Version, Versioned};
use crate::domain::{ChangeDisplayName, RegisterUser, User, UserId};
use app_error::{AppError, AppResult};
use std::sync::Arc;

/// Holds a trait object rather than being generic so the service is one
/// nameable type; one dynamic call per repository method is noise beside the I/O.
#[derive(Clone)]
pub struct UserService {
    repo: Arc<dyn UserRepository>,
}

impl UserService {
    pub fn new(repo: Arc<dyn UserRepository>) -> Self {
        Self { repo }
    }

    pub async fn register(&self, cmd: RegisterUser) -> AppResult<Versioned<User>> {
        let user = User::register(UserId::new(), cmd, app_domain::now())?;
        if self.repo.find_by_email(user.email()).await?.is_some() {
            return Err(AppError::conflict("a user with that email already exists"));
        }
        let version = self.repo.save(&user, Expected::New).await?;
        Ok(Versioned {
            entity: user,
            version,
        })
    }

    /// `expected` comes from `If-Match`; the save is refused if the account has
    /// since changed (e.g. a rename in another tab).
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

    pub async fn exists(&self, id: UserId) -> AppResult<bool> {
        Ok(self.repo.get(id).await?.is_some())
    }

    pub async fn list(&self, query: ListQuery<UserSortKey>) -> AppResult<Listed<Versioned<User>>> {
        self.repo.list(&query).await
    }
}
