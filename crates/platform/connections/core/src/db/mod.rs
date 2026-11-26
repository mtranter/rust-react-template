//! The layer-wide `allow` below covers every file in `db/`.

#![allow(clippy::disallowed_types)]

mod migrations;
mod repository;
mod secrets;

pub use migrations::{MIGRATOR, SCHEMA, migrations};
pub use repository::{ConnectionColumns, PgConnectionRepository};
