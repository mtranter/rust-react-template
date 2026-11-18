//! Run explicitly, never on server startup, so production controls migration
//! timing (ADR 0021). Then registers every module's topics (ADR 0033) and subscriptions (ADR 0014), so
//! they exist before any process of this build publishes or relays, and applies
//! the default grants this build adds (ADR 0040).

use app_domain::SystemClock;
use app_module::Context;
use server::Registered;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = server::config::AppConfig::load()?;
    // Loaded before anything is migrated, so a build whose policies don't load changes nothing.
    server::policies::<Registered>()
        .inspect_err(|err| eprintln!("migrate refused to start: {err}"))?;
    server::telemetry::init(config.env);

    let pool = app_db::connect(&config.database_url, config.db_max_connections).await?;
    app_db::run_migrations(&pool, &server::module_migrations::<Registered>()).await?;
    let ctx = Context::new(pool, Arc::new(SystemClock));
    server::register_topics::<Registered>(&ctx).await?;
    server::register_subscriptions::<Registered>(&ctx).await?;
    let granted = server::apply_default_grants::<Registered>(&ctx).await?;
    tracing::info!(granted, "default grants applied");

    tracing::info!("migrations complete");
    Ok(())
}
