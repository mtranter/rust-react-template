use super::events;
use super::{
    EventPublisher, Expected, ListQuery, Listed, Transactions, UserRepository, UserSortKey,
    Version, Versioned,
};
use crate::domain::{ChangeDisplayName, RegisterUser, User, UserAction, UserId};
use app_authz::{Authorizer, Plan, Principal};
use app_domain::Clock;
use app_error::{AppError, AppResult};
use std::sync::Arc;

/// Holds trait objects rather than being generic so the service is one
/// nameable type; one dynamic call per repository method is noise beside the I/O.
#[derive(Clone)]
pub struct UserService {
    repo: Arc<dyn UserRepository>,
    publisher: Arc<dyn EventPublisher>,
    transactions: Arc<dyn Transactions>,
    clock: Arc<dyn Clock>,
    authorizer: Arc<Authorizer>,
}

impl UserService {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        publisher: Arc<dyn EventPublisher>,
        transactions: Arc<dyn Transactions>,
        clock: Arc<dyn Clock>,
        authorizer: Arc<Authorizer>,
    ) -> Self {
        Self {
            authorizer,
            repo,
            publisher,
            transactions,
            clock,
        }
    }

    /// Asked first in every use case but `register`, before anything is loaded
    /// or begun. A `Where` plan (the `self` scope) is checked against the
    /// account once it is loaded ([`load_for`](Self::load_for)).
    fn authorize(&self, principal: &Principal, action: UserAction) -> AppResult<Plan> {
        self.authorizer.authorize::<User>(principal, action)
    }

    /// Public (`server::PUBLIC`), so it takes no Principal.
    pub async fn register(&self, cmd: RegisterUser) -> AppResult<Versioned<User>> {
        let (user, event) = User::register(UserId::new(), cmd, self.clock.now())?;
        if self.repo.find_by_email(user.email()).await?.is_some() {
            return Err(AppError::conflict("a user with that email already exists"));
        }
        let mut tx = self.transactions.begin().await?;
        let version = self.repo.save(&mut tx, &user, Expected::New).await?;
        self.publisher
            .publish(&mut tx, events::pending(&user, version, event))
            .await?;
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
        principal: &Principal,
        cmd: ChangeDisplayName,
        expected: Version,
    ) -> AppResult<Versioned<User>> {
        let plan = self.authorize(principal, UserAction::Rename)?;
        let mut user = self.load_for(principal, &plan, cmd.user_id).await?.entity;
        let event = user.change_display_name(cmd)?;
        let mut tx = self.transactions.begin().await?;
        let version = self
            .repo
            .save(&mut tx, &user, Expected::Version(expected))
            .await?;
        self.publisher
            .publish(&mut tx, events::pending(&user, version, event))
            .await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: user,
            version,
        })
    }

    /// Removes the account and announces it, as it was last, in the same
    /// transaction; modules holding the id react in their own time (ADR 0011).
    pub async fn delete(
        &self,
        principal: &Principal,
        id: UserId,
        expected: Version,
    ) -> AppResult<()> {
        let plan = self.authorize(principal, UserAction::Delete)?;
        let current = self.load_for(principal, &plan, id).await?;
        // Checked here as well as by the delete, so the state announced is the
        // version deleted, not one read before a concurrent change. The event
        // announces the version after it, so it's the newest of its key.
        if current.version != expected {
            return Err(AppError::precondition_failed(
                "the account was modified elsewhere",
            ));
        }
        let event = current.entity.delete();
        let mut tx = self.transactions.begin().await?;
        self.repo.delete(&mut tx, id, expected).await?;
        self.publisher
            .publish(
                &mut tx,
                events::pending(&current.entity, expected.next(), event),
            )
            .await?;
        tx.commit().await
    }

    pub async fn get(&self, principal: &Principal, id: UserId) -> AppResult<Versioned<User>> {
        let plan = self.authorize(principal, UserAction::View)?;
        self.load_for(principal, &plan, id).await
    }

    /// The account, if `plan` allows it, checked before any domain action.
    async fn load_for(
        &self,
        principal: &Principal,
        plan: &Plan,
        id: UserId,
    ) -> AppResult<Versioned<User>> {
        let user = self
            .repo
            .get(id)
            .await?
            .ok_or_else(|| AppError::not_found("user"))?;
        self.authorizer.check(principal, plan, &user.entity)?;
        Ok(user)
    }

    pub async fn exists(&self, id: UserId) -> AppResult<bool> {
        Ok(self.repo.get(id).await?.is_some())
    }

    pub async fn list(
        &self,
        principal: &Principal,
        query: ListQuery<UserSortKey>,
    ) -> AppResult<Listed<Versioned<User>>> {
        let rows = self.authorizer.rows::<User>(principal, UserAction::List)?;
        self.repo.list(&query, &rows).await
    }
}
