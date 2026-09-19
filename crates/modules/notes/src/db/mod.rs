//! DB layer for notes: the sqlx implementation of the application's
//! `NoteRepository`. The layer-wide `allow` below covers every file in `db/`.
//!
//! - `migrations` — the module's schema name and embedded migration bundle.
//! - `repository` — the Postgres-backed `Repository` impl.

#![allow(clippy::disallowed_types)]

mod migrations;
mod repository;

pub use migrations::{MIGRATOR, SCHEMA, migrations};
pub use repository::PgNoteRepository;
