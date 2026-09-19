//! DB layer: the sqlx implementation of the application's `Repository`. The only
//! place raw SQL for this module lives, and the only layer permitted to name the
//! driver (hence the layer-wide `allow` below, which covers every file in `db/`;
//! the domain and application layers have no such escape hatch).
//!
//! - `migrations` — the module's schema name and embedded migration bundle.
//! - `repository` — the Postgres-backed `Repository` impl.

#![allow(clippy::disallowed_types)]

mod migrations;
mod repository;

pub use migrations::{MIGRATOR, SCHEMA, migrations};
pub use repository::PgUserRepository;
