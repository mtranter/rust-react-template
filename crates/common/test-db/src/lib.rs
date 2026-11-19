//! A Postgres database per test, copied from a template migrated and seeded
//! once per test binary.
//!
//! Replaces `#[sqlx::test]`, which migrates and seeds every test's database
//! from empty and gives all of a binary's tests one pool of 20 connections, so
//! a test holding several (a worker, a listener, a request) waits on the others
//! and can time out.
//!
//! ```ignore
//! pub const TEMPLATE: Template = Template::new("projects", |pool| {
//!     Box::pin(async move { /* migrate and seed `pool` */ })
//! });
//!
//! #[app_test_db::test(crate::driver::TEMPLATE)]
//! async fn a_behaviour(pool: PgPool) { .. }
//! ```
//!
//! A passing test's database is dropped; a failing one's is kept to inspect,
//! until the test next runs (`make test-db-clean` drops them all).

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, Connection, Executor, PgConnection, PgPool};
use std::future::Future;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::pin::Pin;
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::Mutex;

pub use app_test_db_macros::test;

/// Builds a template's database: migrations, seeds, anything every test of a
/// binary starts from.
pub type Build = fn(PgPool) -> Pin<Box<dyn Future<Output = ()>>>;

pub struct Template {
    name: &'static str,
    build: Build,
}

impl Template {
    /// `name` keeps templates apart in the one Postgres: short, `[a-z_]`.
    pub const fn new(name: &'static str, build: Build) -> Self {
        Self { name, build }
    }

    fn database(&self) -> String {
        format!("_test_template_{}", self.name)
    }
}

/// Templates this process has built: rebuilt once per run, so a changed
/// migration or seed is never stale.
static BUILT: Mutex<Vec<&'static str>> = Mutex::const_new(Vec::new());

/// Runs `test` on a fresh copy of `template`, as `#[app_test_db::test]` does.
pub fn run<F, Fut>(test_path: &'static str, template: &'static Template, test: F) -> Fut::Output
where
    F: FnOnce(PgPool) -> Fut,
    Fut: Future,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("start Tokio runtime")
        .block_on(async move {
            ensure_built(template).await;
            let database = copy(template, test_path).await;
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .idle_timeout(Duration::from_secs(1))
                // As `app_db::connect`'s, so a test can measure its waits.
                .acquire_time_level(log::LevelFilter::Debug)
                .connect_with(server().database(&database))
                .await
                .expect("connect to the test's database");

            let output = test(pool.clone()).await;

            if tokio::time::timeout(Duration::from_secs(10), pool.close())
                .await
                .is_err()
            {
                eprintln!("test {test_path} held onto its pool after exiting");
            }
            execute(&format!(
                "drop database if exists {database:?} with (force)"
            ))
            .await;
            output
        })
}

async fn ensure_built(template: &'static Template) {
    let mut built = BUILT.lock().await;
    if built.contains(&template.name) {
        return;
    }
    let database = template.database();
    execute(&format!(
        "drop database if exists {database:?} with (force)"
    ))
    .await;
    execute(&format!("create database {database:?}")).await;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_with(server().database(&database))
        .await
        .expect("connect to the template database");
    (template.build)(pool.clone()).await;
    // A database can't be copied while anyone is connected to it.
    pool.close().await;
    built.push(template.name);
}

async fn copy(template: &Template, test_path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    test_path.hash(&mut hasher);
    let database = format!("_test_{}_{:016x}", template.name, hasher.finish());
    execute(&format!(
        "drop database if exists {database:?} with (force)"
    ))
    .await;
    execute(&format!(
        "create database {database:?} template {:?}",
        template.database()
    ))
    .await;
    database
}

/// `DATABASE_URL` names the server, and the database to manage the others from.
fn server() -> PgConnectOptions {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnectOptions::from_str(&url).expect("DATABASE_URL is a Postgres URL")
}

async fn execute(sql: &str) {
    let mut conn = PgConnection::connect_with(&server())
        .await
        .expect("connect to DATABASE_URL");
    conn.execute(AssertSqlSafe(sql.to_string()))
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"));
    let _ = conn.close().await;
}
