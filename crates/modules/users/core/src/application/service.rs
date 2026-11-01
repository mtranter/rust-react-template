use super::events;
use super::{
    Expected, ListQuery, Listed, Transactions, UserRepository, UserSortKey, Version, Versioned,
};
use crate::domain::{ChangeDisplayName, RegisterUser, User, UserId};
use app_domain::Clock;
use app_error::{AppError, AppResult};
use app_events::EventPayload;
use std::sync::Arc;
use users_api::events::v1::{UserDeleted, UserDisplayNameChanged, UserRegistered};

/// Holds trait objects rather than being generic so the service is one
/// nameable type; one dynamic call per repository method is noise beside the I/O.
#[derive(Clone)]
pub struct UserService {
    repo: Arc<dyn UserRepository>,
    transactions: Arc<dyn Transactions>,
    clock: Arc<dyn Clock>,
}

impl UserService {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        transactions: Arc<dyn Transactions>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            transactions,
            clock,
        }
    }

    pub async fn register(&self, cmd: RegisterUser) -> AppResult<Versioned<User>> {
        let user = User::register(UserId::new(), cmd, self.clock.now())?;
        if self.repo.find_by_email(user.email()).await?.is_some() {
            return Err(AppError::conflict("a user with that email already exists"));
        }
        let mut tx = self.transactions.begin().await?;
        let version = self.repo.save(&mut tx, &user, Expected::New).await?;
        tx.publish::<UserRegistered>(EventPayload::new(events::user(&user)))?;
        tx.commit().await?;
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
        let mut tx = self.transactions.begin().await?;
        let version = self
            .repo
            .save(&mut tx, &user, Expected::Version(expected))
            .await?;
        tx.publish::<UserDisplayNameChanged>(EventPayload::new(events::user(&user)))?;
        tx.commit().await?;
        Ok(Versioned {
            entity: user,
            version,
        })
    }

    /// Removes the account and announces it, as it was last, in the same
    /// transaction; modules holding the id react in their own time (spec §4).
    pub async fn delete(&self, id: UserId, expected: Version) -> AppResult<()> {
        let current = self.get(id).await?;
        // Checked here as well as by the delete, so the state announced is the
        // version deleted, not one read before a concurrent change.
        if current.version != expected {
            return Err(AppError::precondition_failed(
                "the account was modified elsewhere",
            ));
        }
        let mut tx = self.transactions.begin().await?;
        self.repo.delete(&mut tx, id, expected).await?;
        tx.publish::<UserDeleted>(EventPayload::new(events::user(&current.entity)))?;
        tx.commit().await
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
