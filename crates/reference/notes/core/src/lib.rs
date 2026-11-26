//! Uses the users module only through the pure `users-api` crate (ADR 0008).

mod application;
mod db;
mod di;
mod domain;
#[cfg(feature = "seed")]
pub mod seed;
mod web;

pub use application::{NOTICEBOARD, ShareNote, noticeboard};
pub use db::{MIGRATOR, NoteColumns, SCHEMA, migrations};
pub use di::{NotesProvider, handlers, module, policies, provider};
pub use domain::{
    AuthorId, CreateNote, EditNote, Item, ItemId, Note, NoteAction, NoteError, NoteId,
};
pub use web::NotesEndpoints;
