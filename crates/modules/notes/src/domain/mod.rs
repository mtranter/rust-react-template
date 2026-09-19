//! Domain layer for notes: aggregates, commands, and domain errors. Pure — no
//! poem, no sqlx, no repositories, no async.
//!
//! Note that [`AuthorId`] is the notes module's **own** newtype, not a
//! `users::UserId`: modules stay isolated and independently liftable (spec §D1).
//! There is no foreign key to the users module; cross-module referential
//! integrity, if ever needed, is an application-level concern.
//!
//! - `ids` — strongly-typed identifiers.
//! - `note` — the `Note` aggregate and its domain actions.
//! - `commands` — the intents a domain action accepts.
//! - `error` — the module's domain error enum.

mod commands;
mod error;
mod ids;
mod note;

pub use commands::{CreateNote, EditNote};
pub use error::NoteError;
pub use ids::{AuthorId, NoteId};
pub use note::Note;
