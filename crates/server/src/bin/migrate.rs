//! Run explicitly, never on server startup, so production controls migration
//! timing (spec §6). Then registers every module's subscriptions (spec §4), so
//! they exist before any process of this build publishes or relays.

use app_domain::SystemClock;
use app_module::Context;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = server::config::AppConfig::load()?;
    server::telemetry::init(config.env);

    let pool = app_db::connect(&config.database_url, config.db_max_connections).await?;
    app_db::run_migrations(&pool, &server::module_migrations()).await?;
    server::register_subscriptions(&Context::new(pool, Arc::new(SystemClock))).await?;

    tracing::info!("migrations complete");
    Ok(())
}
