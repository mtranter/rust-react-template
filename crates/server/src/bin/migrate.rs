//! Run explicitly, never on server startup, so production controls migration
//! timing (ADR 0021). Then registers every module's topics (ADR 0033) and subscriptions (ADR 0014), so
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
    let ctx = Context::new(pool, Arc::new(SystemClock));
    server::register_topics(&ctx).await?;
    server::register_subscriptions(&ctx).await?;

    tracing::info!("migrations complete");
    Ok(())
}
