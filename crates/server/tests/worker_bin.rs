//! The `worker` binary's exits, run as a deployment would run it.

use std::process::{Command, Output};

fn worker(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_worker"))
        .args(args)
        // `.invalid` never resolves (RFC 6761), so this fails at once, where a
        // refused connection is retried for the pool's 30 s acquire timeout.
        .env("DATABASE_URL", "postgres://app:app@db.invalid/app")
        .env("APP_REDIS_URL", "redis://bus.invalid")
        .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
        .output()
        .expect("run the worker")
}

fn printed(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr)
}

#[test]
fn a_budget_without_drain_is_a_usage_error() {
    let output = worker(&["--max-seconds", "5"]);

    assert_eq!(output.status.code(), Some(2), "{}", printed(&output));
    assert!(printed(&output).contains("--max-seconds needs --drain"));
}

#[test]
fn a_drain_that_cannot_reach_the_database_fails_saying_why() {
    let output = worker(&["--drain", "--max-seconds", "5"]);

    assert!(!output.status.success(), "{}", printed(&output));
    assert!(
        printed(&output).contains("worker can't reach the database"),
        "{}",
        printed(&output)
    );
}
