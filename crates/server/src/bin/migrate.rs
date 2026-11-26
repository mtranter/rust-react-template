//! Run explicitly, never on server startup, so production controls migration
//! timing (ADR 0021). Then registers every module's topics (ADR 0033),
//! subscriptions (ADR 0014) and schedules (ADR 0043), so they exist before any
//! process of this build publishes, relays or schedules, applies the default
//! grants this build adds (ADR 0040) and records the Connections its modules
//! declare (ADR 0050).

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
    let telemetry = server::telemetry::init(config.env, "migrate");

    let pool = app_db::connect(&config.database_url, config.db_max_connections).await?;
    let ctx = Context::new(pool.clone(), Arc::new(SystemClock));
    // Before anything is migrated, as the policies are.
    server::check_schedules::<Registered>(&ctx)
        .inspect_err(|err| tracing::error!(error = %err, "migrate refused to start"))?;
    app_db::run_migrations(&pool, &server::module_migrations::<Registered>()).await?;
    server::register_topics::<Registered>(&ctx).await?;
    server::register_subscriptions::<Registered>(&ctx).await?;
    server::register_schedules::<Registered>(&ctx).await?;
    let granted = server::apply_default_grants::<Registered>(&ctx).await?;
    tracing::info!(granted, "default grants applied");
    let declared = server::declare_connections::<Registered>(&ctx).await?;
    tracing::info!(declared, "connections declared");

    tracing::info!("migrations complete");
    telemetry.shutdown();
    Ok(())
}
