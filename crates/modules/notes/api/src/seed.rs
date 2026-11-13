//! The notes module's seed data as other modules may know it (ADR 0028). Ids are
//! `well_known_id(MODULE, kind, sequence)`; kind `1` is a note, kind `2` an
//! author. `notes::seed` saves it.

use crate::v1;
use app_domain::{seed_time, well_known_id};
use users_api::seed::{USER_ID_ALICE, USER_ID_BOB};
use uuid::Uuid;

/// Unique across modules; `make check-modules` enforces it.
pub const MODULE: u32 = 2;

/// No user has this id: stands in for an author since removed, as notes holds
/// no foreign key to users.
pub const AUTHOR_ID_UNKNOWN: Uuid = well_known_id(MODULE, 2, 1);

// Notes 1 and 2 were an earlier seed's. Never reuse a seed id for other data:
// an environment already seeded keeps the old row, since seeding skips ids it has.
pub const ALICE_NOTE_ID_SHOPPING_LIST: Uuid = well_known_id(MODULE, 1, 3);
pub const BOB_NOTE_ID_BIKE_REPAIRS: Uuid = well_known_id(MODULE, 1, 4);
pub const NOTE_ID_ORPHANED: Uuid = well_known_id(MODULE, 1, 5);

fn note(id: Uuid, author_id: Uuid, title: &str, minutes: i64) -> v1::Note {
    v1::Note {
        id,
        author_id,
        title: title.into(),
        created_at: seed_time(minutes),
    }
}

pub fn alices_note() -> v1::Note {
    note(
        ALICE_NOTE_ID_SHOPPING_LIST,
        USER_ID_ALICE,
        "Shopping list",
        0,
    )
}

pub fn bobs_note() -> v1::Note {
    note(BOB_NOTE_ID_BIKE_REPAIRS, USER_ID_BOB, "Bike repairs", 1)
}

pub fn orphaned_note() -> v1::Note {
    note(NOTE_ID_ORPHANED, AUTHOR_ID_UNKNOWN, "Holiday shopping", 2)
}
