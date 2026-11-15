//! The layer-wide `allow` below covers every file in `db/`.

#![allow(clippy::disallowed_types)]

mod finder;
pub mod migrations;
mod projection;

pub use finder::PgEntryFinder;
pub use projection::{COPY, EntryRows};
