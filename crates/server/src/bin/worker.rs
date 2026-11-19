//! Delivers every module's events to their subscribers (ADR 0013), beside the
//! server and from the same image. Run as many as the load needs: they share
//! the queue without running a job twice. With `--drain`, it runs what is due
//! and exits, for an external timer (cron, a container scheduler) to drive.

use app_domain::SystemClock;
use app_error::AppError;
use app_module::Context;
use chrono::TimeDelta;
use std::sync::Arc;
use std::time::Duration;

const USAGE: &str = "usage: worker [--drain [--max-seconds N]]";

#[derive(Debug, PartialEq)]
enum Mode {
    Poll,
    Drain { budget: Option<Duration> },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = mode(std::env::args().skip(1)).unwrap_or_else(|err| {
        eprintln!("worker: {err}\n{USAGE}");
        std::process::exit(2);
    });
    let config = server::config::AppConfig::load()?;
    let authorizer = server::authorizer::<server::Registered>()
        .inspect_err(|err| eprintln!("worker refused to start: {err}"))?;
    let bus = config.bus(server::uses_bus::<server::Registered>())?;
    let telemetry = server::telemetry::init(config.env, "worker");

    let pool = app_db::connect(&config.database_url, config.db_max_connections)
        .await
        .inspect_err(
            |err| tracing::error!(error = %cause(err), "worker can't reach the database"),
        )?;
    let compact_every = TimeDelta::seconds(config.outbox_compact_every as i64);
    let ctx = Context::new(pool, Arc::new(SystemClock))
        .with_bus(bus)
        .with_authorizer(Arc::new(authorizer));
    server::check_schedules::<server::Registered>(&ctx)
        .inspect_err(|err| tracing::error!(error = %err, "worker refused to start"))?;
    let worker = server::worker::<server::Registered>(&ctx)
        .compacting_every(compact_every)
        .checking_its_clock();
    if let Err(err) = worker.check_clock().await {
        tracing::error!(error = %cause(&err), "worker refused to start");
        return Err(err.into());
    }

    match mode {
        Mode::Poll => {
            let interval = Duration::from_millis(config.worker_poll_ms);
            tracing::info!(?interval, %compact_every, "starting worker");
            worker.run(interval, server::shutdown_signal()).await?;
            tracing::info!("worker stopped");
        }
        Mode::Drain { budget } => {
            tracing::info!(?budget, "draining");
            worker
                .drain(budget)
                .await
                .inspect_err(|err| tracing::error!(error = %cause(err), "worker drain failed"))?;
            tracing::info!("drained");
        }
    }
    telemetry.shutdown();
    Ok(())
}

fn mode(args: impl IntoIterator<Item = String>) -> Result<Mode, String> {
    let mut drain = false;
    let mut budget = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--drain" => drain = true,
            "--max-seconds" => {
                let seconds = args
                    .next()
                    .ok_or("--max-seconds needs a number of seconds")?;
                let seconds: u64 = seconds
                    .parse()
                    .map_err(|_| format!("--max-seconds takes whole seconds, not `{seconds}`"))?;
                budget = Some(Duration::from_secs(seconds));
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    match (drain, budget) {
        (true, budget) => Ok(Mode::Drain { budget }),
        (false, None) => Ok(Mode::Poll),
        (false, Some(_)) => Err("--max-seconds needs --drain".into()),
    }
}

/// `AppError::Internal` displays only "internal error".
fn cause(err: &AppError) -> String {
    match err {
        AppError::Internal(source) => source.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Mode, String> {
        mode(args.iter().map(|a| a.to_string()))
    }

    #[test]
    fn no_flags_poll() {
        assert_eq!(parse(&[]), Ok(Mode::Poll));
    }

    #[test]
    fn drain_takes_an_optional_budget_in_seconds() {
        assert_eq!(parse(&["--drain"]), Ok(Mode::Drain { budget: None }));
        assert_eq!(
            parse(&["--drain", "--max-seconds", "50"]),
            Ok(Mode::Drain {
                budget: Some(Duration::from_secs(50))
            })
        );
        assert_eq!(
            parse(&["--max-seconds", "50", "--drain"]),
            Ok(Mode::Drain {
                budget: Some(Duration::from_secs(50))
            })
        );
    }

    #[test]
    fn a_budget_without_drain_is_refused() {
        assert!(parse(&["--max-seconds", "50"]).is_err());
    }

    #[test]
    fn malformed_arguments_are_refused() {
        assert!(parse(&["--drain", "--max-seconds"]).is_err());
        assert!(parse(&["--drain", "--max-seconds", "-1"]).is_err());
        assert!(parse(&["--drain", "--max-seconds", "soon"]).is_err());
        assert!(parse(&["--poll"]).is_err());
    }
}
