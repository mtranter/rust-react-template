//! What other modules may know of `notes` (spec D7). Notes are referred to by
//! plain `Uuid`: `NoteId` stays private to the notes domain.

#[cfg(feature = "seed")]
pub mod seed;
pub mod v1;
