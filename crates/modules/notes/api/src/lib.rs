//! What other modules may know of `notes` (ADR 0008). Notes are referred to by
//! plain `Uuid`: `NoteId` stays private to the notes domain.

pub mod events;
#[cfg(feature = "seed")]
pub mod seed;
pub mod v1;
