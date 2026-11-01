//! Uses the users module only through the pure `users-api` crate (spec D7).

mod application;
mod db;
mod di;
mod domain;
#[cfg(feature = "seed")]
pub mod seed;
mod web;

pub use db::{MIGRATOR, SCHEMA, migrations};
pub use di::{NotesProvider, handlers, module, provider};
pub use domain::{AuthorId, CreateNote, EditNote, Note, NoteError, NoteId};
pub use web::NotesEndpoints;
