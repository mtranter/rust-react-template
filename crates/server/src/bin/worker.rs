//! Delivers every module's events to their subscribers (ADR 0013), beside the
//! server and from the same image. Run as many as the load needs: they share
//! the queue without running a job twice.

use app_domain::SystemClock;
use app_error::AppError;
use app_module::Context;
use chrono::TimeDelta;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = server::config::AppConfig::load()?;
    let authorizer = server::authorizer::<server::Registered>()
        .inspect_err(|err| eprintln!("worker refused to start: {err}"))?;
    let bus = config.bus(server::uses_bus::<server::Registered>())?;
    let telemetry = server::telemetry::init(config.env, "worker");

    let pool = app_db::connect(&config.database_url, config.db_max_connections).await?;
    let compact_every = TimeDelta::seconds(config.outbox_compact_every as i64);
    let worker = server::worker::<server::Registered>(
        &Context::new(pool, Arc::new(SystemClock))
            .with_bus(bus)
            .with_authorizer(Arc::new(authorizer)),
    )
    .compacting_every(compact_every)
    .checking_its_clock();
    if let Err(err) = worker.check_clock().await {
        let cause = match &err {
            AppError::Internal(source) => source.to_string(),
            other => other.to_string(),
        };
        tracing::error!(error = %cause, "worker refused to start");
        return Err(err.into());
    }

    let interval = Duration::from_millis(config.worker_poll_ms);
    tracing::info!(?interval, %compact_every, "starting worker");
    worker.run(interval, server::shutdown_signal()).await?;
    tracing::info!("worker stopped");
    telemetry.shutdown();
    Ok(())
}
