//! The module's schema and embedded migrations.

use app_db::ModuleMigrations;
use sqlx::migrate::Migrator;

/// The Postgres schema this module owns.
pub const SCHEMA: &str = "users";

/// Migrations embedded from `modules/users/migrations/` at compile time.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// This module's migration bundle, for the server/test migration runner.
pub fn migrations() -> ModuleMigrations {
    ModuleMigrations {
        schema: SCHEMA,
        migrator: &MIGRATOR,
    }
}
