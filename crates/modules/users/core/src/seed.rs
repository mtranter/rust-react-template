//! Builds the users `users_api::seed` describes, adding what isn't published.

// Seeding wires a pool to a repository, like `di.rs`.
#![allow(clippy::disallowed_types)]

use crate::application::{Expected, Transactions, UserRepository};
use crate::db::PgUserRepository;
use crate::domain::{RegisterUser, User, UserId};
use app_db::PgTransactions;
use app_domain::{SystemClock, seed_time};
use app_error::AppResult;
use sqlx::PgPool;
use std::sync::Arc;
use users_api::{seed, v1};

pub const ALICE: UserId = UserId::from_uuid(seed::ALICE);
pub const BOB: UserId = UserId::from_uuid(seed::BOB);

fn user(published: v1::User, email: &str, minutes: i64) -> User {
    User::register(
        UserId::from_uuid(published.id),
        RegisterUser {
            email: email.into(),
            display_name: published.display_name,
        },
        seed_time(minutes),
    )
    .expect("seed data satisfies the domain's invariants")
}

pub fn alice() -> User {
    user(seed::alice(), "alice@example.com", 0)
}

pub fn bob() -> User {
    user(seed::bob(), "bob@example.com", 1)
}

pub fn users() -> Vec<User> {
    vec![alice(), bob()]
}

/// Saves only what's missing, so a deployed environment can re-run it on every
/// release without failing or undoing edits to seeded entities. Saves through the
/// repository, not the service, so seeded users get no welcome email.
pub async fn standard(pool: &PgPool) -> AppResult<()> {
    let repo = PgUserRepository::new(pool.clone());
    let mut tx = PgTransactions::new(pool.clone(), Arc::new(SystemClock))
        .begin()
        .await?;
    for user in users() {
        if repo.get(user.id()).await?.is_none() {
            repo.save(&mut tx, &user, Expected::New).await?;
        }
    }
    tx.commit().await
}
