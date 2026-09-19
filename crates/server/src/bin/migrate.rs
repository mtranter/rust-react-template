//! Apply every module's migrations in order. Run explicitly (`make migrate`),
//! never on server startup, so production controls migration timing (spec §6).

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = app_config::AppConfig::load()?;
    app_telemetry::init(config.env);

    let pool = app_db::connect(&config.database_url, config.db_max_connections).await?;
    app_db::run_migrations(&pool, &server::module_migrations()).await?;

    tracing::info!("migrations complete");
    Ok(())
}
