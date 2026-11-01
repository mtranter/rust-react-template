//! The notes module's seed data as other modules may know it (spec §13). Ids are
//! `well_known_id(MODULE, kind, sequence)`; kind `1` is a note, kind `2` an
//! author. `notes::seed` saves it.

use crate::v1;
use app_domain::{seed_time, well_known_id};
use users_api::seed::{ALICE, BOB};
use uuid::Uuid;

/// Unique across modules; `make check-modules` enforces it.
pub const MODULE: u32 = 2;

/// No user has this id: stands in for an author since removed, as notes holds
/// no foreign key to users.
pub const UNKNOWN_AUTHOR: Uuid = well_known_id(MODULE, 2, 1);

// Notes 1 and 2 were an earlier seed's. Never reuse a seed id for other data:
// an environment already seeded keeps the old row, since seeding skips ids it has.
pub const ALICES_NOTE: Uuid = well_known_id(MODULE, 1, 3);
pub const BOBS_NOTE: Uuid = well_known_id(MODULE, 1, 4);
pub const ORPHANED_NOTE: Uuid = well_known_id(MODULE, 1, 5);

fn note(id: Uuid, author_id: Uuid, title: &str, minutes: i64) -> v1::Note {
    v1::Note {
        id,
        author_id,
        title: title.into(),
        created_at: seed_time(minutes),
    }
}

pub fn alices_note() -> v1::Note {
    note(ALICES_NOTE, ALICE, "Shopping list", 0)
}

pub fn bobs_note() -> v1::Note {
    note(BOBS_NOTE, BOB, "Bike repairs", 1)
}

pub fn orphaned_note() -> v1::Note {
    note(ORPHANED_NOTE, UNKNOWN_AUTHOR, "Holiday shopping", 2)
}
