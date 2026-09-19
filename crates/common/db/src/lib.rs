//! `app-db` — the sqlx-coupled persistence support crate.
//!
//! Owns `PgPool` construction and the schema-per-module migration runner. Only
//! db layers (`src/db/`) and the server depend on this; `cargo-deny` forbids any
//! other crate from reaching sqlx (spec §4).
//!
//! ## Repository convention
//!
//! Each module's **application** layer defines a `Repository` trait; its **db**
//! layer implements it over a `PgPool`, using `sqlx::query!` with **schema-qualified**
//! table names (e.g. `FROM users.account`). Qualifying names means compile-time
//! query checking works against the default search path, while each module's
//! tables stay isolated in its own Postgres schema.

use app_error::{AppError, AppResult};
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, PgPool};

/// Re-exported so db-coupled callers (the server's readiness route, module db
/// layers) can name the pool type without taking their own `sqlx` dependency.
pub use sqlx::PgPool as Pool;

/// One module's migration bundle: the Postgres schema it owns and its embedded
/// `Migrator` (from `sqlx::migrate!()` in the module crate).
pub struct ModuleMigrations {
    pub schema: &'static str,
    pub migrator: &'static Migrator,
}

/// Build a connection pool. Callers pass the resolved `database_url` and pool
/// size from config.
pub async fn connect(database_url: &str, max_connections: u32) -> AppResult<PgPool> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
        .map_err(AppError::internal)
}

/// Liveness check for the readiness endpoint.
pub async fn ping(pool: &PgPool) -> AppResult<()> {
    sqlx::query("SELECT 1")
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(AppError::internal)
}

/// Run every module's migrations, each isolated in its own schema.
///
/// For each module we create its schema and set the connection `search_path` to
/// it before running the migrator, so the `_sqlx_migrations` bookkeeping table
/// lives in that schema — modules can therefore version independently without
/// colliding. Runs in a deterministic order (the order of `modules`).
pub async fn run_migrations(pool: &PgPool, modules: &[ModuleMigrations]) -> AppResult<()> {
    let mut conn = pool.acquire().await.map_err(AppError::internal)?;

    for module in modules {
        tracing::info!(schema = module.schema, "running migrations");

        // Schema name is a trusted `&'static str` from module code, not user
        // input — safe to assert. sqlx 0.9 requires the explicit audit wrapper.
        sqlx::query(AssertSqlSafe(format!(
            "CREATE SCHEMA IF NOT EXISTS \"{}\"",
            module.schema
        )))
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;

        // Session-level: persists across the migrator's per-migration transactions.
        sqlx::query(AssertSqlSafe(format!(
            "SET search_path TO \"{}\"",
            module.schema
        )))
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;

        module
            .migrator
            .run(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    }

    // Leave the (pooled) connection with a neutral search_path.
    sqlx::query("SET search_path TO \"$user\", public")
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;

    Ok(())
}
