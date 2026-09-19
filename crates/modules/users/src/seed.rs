//! Seed data: a deterministic, well-known dataset for tests (and, later, local
//! development). Compiled only with the `seed` feature, so it never ships in a
//! production build.
//!
//! Every seeded entity has a **named, stable id** and a fixed timestamp, so
//! tests refer to `seed::ALICE` rather than capturing ids at runtime, and list
//! ordering is reproducible. Seeds are built through the real domain
//! constructors (so they satisfy every invariant) and written through the real
//! repository (so there is no second copy of the SQL).
//!
//! Id convention (spec §13): `MMMMMMMM-KKKK-0000-0000-NNNNNNNNNNNN` — module
//! number, entity kind within the module, sequence. `users` is module `1`.

// Seeding is composition, like the api layer: it wires a pool to a repository.
#![allow(clippy::disallowed_types)]

use crate::application::{Expected, UserRepository};
use crate::db::PgUserRepository;
use crate::domain::{RegisterUser, User, UserId};
use app_error::AppResult;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Alice — `00000001-0001-0000-0000-000000000001`.
pub const ALICE: UserId =
    UserId::from_uuid(Uuid::from_u128(0x00000001_0001_0000_0000_000000000001));
/// Bob — `00000001-0001-0000-0000-000000000002`.
pub const BOB: UserId = UserId::from_uuid(Uuid::from_u128(0x00000001_0001_0000_0000_000000000002));

/// `2025-01-01T00:00:00Z` plus `minutes`. Seeds are spaced a minute apart so
/// "most-recent first" ordering is deterministic.
fn at(minutes: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_735_689_600 + minutes * 60, 0).expect("valid seed timestamp")
}

fn user(id: UserId, email: &str, display_name: &str, minutes: i64) -> User {
    User::register(
        id,
        RegisterUser {
            email: email.into(),
            display_name: display_name.into(),
        },
        at(minutes),
    )
    .expect("seed data satisfies the domain's invariants")
}

/// `alice@example.com`, "Alice".
pub fn alice() -> User {
    user(ALICE, "alice@example.com", "Alice", 0)
}

/// `bob@example.com`, "Bob". Created after Alice.
pub fn bob() -> User {
    user(BOB, "bob@example.com", "Bob", 1)
}

/// Every user in the standard dataset, oldest first.
pub fn users() -> Vec<User> {
    vec![alice(), bob()]
}

/// Persist the standard dataset into a migrated database.
pub async fn standard(pool: &PgPool) -> AppResult<()> {
    let repo = PgUserRepository::new(pool.clone());
    for user in users() {
        repo.save(&user, Expected::New).await?;
    }
    Ok(())
}
