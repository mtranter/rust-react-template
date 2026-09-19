//! `notes` module — proves cross-module isolation (its own crate, own schema,
//! no dependency on `users`). Same four-layer shape as `users`.

mod api;
mod application;
mod db;
mod di;
mod domain;
#[cfg(feature = "seed")]
pub mod seed;

pub use api::NotesApi;
pub use db::{MIGRATOR, SCHEMA, migrations};
pub use di::{NotesProvider, provider};
pub use domain::{AuthorId, CreateNote, EditNote, Note, NoteError, NoteId};
