//! The layer-wide `allow` below covers every file in `db/`.

#![allow(clippy::disallowed_types)]

mod migrations;
mod repository;
mod search;

pub use migrations::{MIGRATOR, SCHEMA, migrations};
pub use repository::PgWidgetRepository;
pub use search::{PgWidgetSearch, SEARCH, SearchRows};
