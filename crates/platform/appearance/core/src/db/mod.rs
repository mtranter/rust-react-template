//! The layer-wide `allow` below covers every file in `db/`.

#![allow(clippy::disallowed_types)]

mod app_visibility;
mod migrations;
mod repository;

pub use app_visibility::PgAppVisibilityRepository;
pub use migrations::{MIGRATOR, SCHEMA, migrations};
pub use repository::PgThemeRepository;
