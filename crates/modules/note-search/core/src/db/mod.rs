//! The layer-wide `allow` below covers every file in `db/`.

#![allow(clippy::disallowed_types)]

mod finder;

pub use finder::PgNoteFinder;
