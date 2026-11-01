//! Delivers every module's events and runs their jobs (spec §4), beside the
//! server and from the same image. Run as many as the load needs: they share
//! the queue without running a job twice.

use app_domain::SystemClock;
use app_module::Context;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = server::config::AppConfig::load()?;
    server::telemetry::init(config.env);

    let pool = app_db::connect(&config.database_url, config.db_max_connections).await?;
    let worker = server::worker(&Context::new(pool, Arc::new(SystemClock)));

    let interval = Duration::from_millis(config.worker_poll_ms);
    tracing::info!(?interval, "starting worker");
    worker.run(interval, server::shutdown_signal()).await;
    tracing::info!("worker stopped");
    Ok(())
}
