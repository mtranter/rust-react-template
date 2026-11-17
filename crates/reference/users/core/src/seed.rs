//! Builds the users `users_api::seed` describes, adding what isn't published
//! (when they registered).

// Seeding wires a pool to a repository, like `di.rs`.
#![allow(clippy::disallowed_types)]

use crate::application::{EventPublisher, Expected, Transactions, UserRepository, pending};
use crate::db::PgUserRepository;
use crate::domain::{RegisterUser, User, UserEvent, UserId};
use app_db::{PgEventPublisher, PgTransactions};
use app_domain::{FixedClock, seed_time};
use app_error::AppResult;
use sqlx::PgPool;
use std::sync::Arc;
use users_api::{seed, v1};

pub const USER_ID_ALICE: UserId = UserId::from_uuid(seed::USER_ID_ALICE);
pub const USER_ID_BOB: UserId = UserId::from_uuid(seed::USER_ID_BOB);

fn user(published: v1::User, minutes: i64) -> User {
    User::register(
        UserId::from_uuid(published.id),
        RegisterUser {
            email: published.email,
            display_name: published.display_name,
        },
        seed_time(minutes),
    )
    .expect("seed data satisfies the domain's invariants")
    .0
}

pub fn alice() -> User {
    user(seed::alice(), 0)
}

pub fn bob() -> User {
    user(seed::bob(), 1)
}

pub fn users() -> Vec<User> {
    vec![alice(), bob()]
}

/// Saves only what's missing, so a deployed environment can re-run it on every
/// release without failing or undoing edits to seeded entities. Announces each
/// user it saves as registered, as of then, so a copy kept from the topic has
/// them too; the worker welcomes them like any new user.
pub async fn standard(pool: &PgPool) -> AppResult<()> {
    let repo = PgUserRepository::new(pool.clone());
    let mut tx = PgTransactions::new(pool.clone()).begin().await?;
    for user in users() {
        if repo.get(user.id()).await?.is_none() {
            let version = repo.save(&mut tx, &user, Expected::New).await?;
            PgEventPublisher::new(Arc::new(FixedClock::new(user.created_at())))
                .publish(&mut tx, pending(&user, version, UserEvent::Registered))
                .await?;
        }
    }
    tx.commit().await
}
