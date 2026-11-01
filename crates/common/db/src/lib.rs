//! Repositories use **schema-qualified** table names (`FROM users.account`) so
//! compile-time `query!` checking works against the default search path while
//! each module keeps its own schema.

mod errors;
mod listing;
mod publishing;
mod unit_of_work;
mod worker;

pub use errors::map_db_error;
pub use listing::push_list_clauses;
pub use publishing::PgEventPublisher;
pub use unit_of_work::{PgTransactions, conn};
pub use worker::{DeadLetter, Worker};

use app_error::{AppError, AppResult};
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, PgPool};

/// Lets callers name the pool without their own `sqlx` dependency.
pub use sqlx::PgPool as Pool;

#[derive(Clone, Copy)]
pub struct ModuleMigrations {
    pub schema: &'static str,
    pub migrator: &'static Migrator,
}

pub async fn connect(database_url: &str, max_connections: u32) -> AppResult<PgPool> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
        .map_err(AppError::internal)
}

pub async fn ping(pool: &PgPool) -> AppResult<()> {
    sqlx::query("SELECT 1")
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(AppError::internal)
}

static MESSAGING: Migrator = sqlx::migrate!("./migrations");

/// Runs each migrator with `search_path` set to the module's schema, so each
/// module gets its own `_sqlx_migrations` table and versions independently.
/// The `messaging` schema (the outbox and job queue every module writes) always
/// comes first.
pub async fn run_migrations(pool: &PgPool, modules: &[ModuleMigrations]) -> AppResult<()> {
    let mut conn = pool.acquire().await.map_err(AppError::internal)?;
    let messaging = ModuleMigrations {
        schema: "messaging",
        migrator: &MESSAGING,
    };

    for module in std::iter::once(&messaging).chain(modules) {
        tracing::info!(schema = module.schema, "running migrations");

        // Safe: the schema is a `&'static str` from module code, not user input.
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

    // The connection goes back to the pool; don't leak the last schema.
    sqlx::query("SET search_path TO \"$user\", public")
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;

    Ok(())
}
