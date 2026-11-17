//! The users module's seed data as other modules may know it (ADR 0028). Ids are
//! `well_known_id(MODULE, kind, sequence)`; kind `1` is a user. `users::seed`
//! saves it.

use crate::v1;
use app_domain::well_known_id;
use uuid::Uuid;

/// Unique across modules; `make check-modules` enforces it.
pub const MODULE: u32 = 1;

pub const USER_ID_ALICE: Uuid = well_known_id(MODULE, 1, 1);
pub const USER_ID_BOB: Uuid = well_known_id(MODULE, 1, 2);

fn user(id: Uuid, display_name: &str, email: &str) -> v1::User {
    v1::User {
        id,
        display_name: display_name.into(),
        email: email.into(),
    }
}

pub fn alice() -> v1::User {
    user(USER_ID_ALICE, "Alice", "alice@example.com")
}

pub fn bob() -> v1::User {
    user(USER_ID_BOB, "Bob", "bob@example.com")
}
