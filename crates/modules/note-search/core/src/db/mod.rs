//! The layer-wide `allow` below covers every file in `db/`.

#![allow(clippy::disallowed_types)]

mod finder;
pub mod migrations;
mod projections;

pub use finder::PgNoteFinder;
pub use projections::{AuthorRows, COPY_AUTHORS, COPY_NOTES, NoteRows};
