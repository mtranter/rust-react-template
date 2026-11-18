//! Loads every module's standard seed data into a non-prod environment. Run it
//! after `migrate`, as a one-off task inside the environment's network.

use app_domain::SystemClock;
use app_module::Context;
use server::config::{AppConfig, Environment};
use server::{Composition, Registered};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load()?;
    server::telemetry::init(config.env);
    if config.env == Environment::Prod {
        return Err("refusing to seed a prod environment".into());
    }

    let pool = app_db::connect(&config.database_url, config.db_max_connections).await?;
    let ctx = Context::new(pool, Arc::new(SystemClock));
    for module in Registered::modules() {
        module.seed(&ctx).await?;
    }

    tracing::info!("seed complete");
    Ok(())
}
