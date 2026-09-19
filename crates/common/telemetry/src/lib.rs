//! Tracing/telemetry initialization.
//!
//! JSON output in prod (machine-ingestable), pretty output in dev. The log level
//! is driven by `RUST_LOG` (falling back to a sensible default). OpenTelemetry
//! export is left as a documented seam below — wire an OTLP layer here.

use app_config::Environment;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Initialize the global tracing subscriber for the given environment.
///
/// Idempotency: call once at startup. Safe to call from `main` before any async
/// runtime is started. Tests that need tracing can call [`init`] with
/// [`Environment::Test`]; a second call is ignored by `set_global_default`.
pub fn init(env: Environment) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));

    let registry = tracing_subscriber::registry().with(filter);

    match env {
        Environment::Prod => {
            // Structured JSON for log aggregation.
            let _ = registry
                .with(fmt::layer().json().with_current_span(true))
                .try_init();
        }
        Environment::Dev | Environment::Test => {
            // Human-readable, colourized.
            let _ = registry.with(fmt::layer().pretty()).try_init();
        }
    }

    // --- OpenTelemetry seam -------------------------------------------------
    // To export spans, add `opentelemetry` + `tracing-opentelemetry` and layer
    // an OTLP exporter onto the registry above, e.g.:
    //
    //   let otel = tracing_opentelemetry::layer().with_tracer(tracer);
    //   registry.with(otel).with(fmt_layer).try_init();
    //
    // Left uninstalled by default (non-goal per spec §1).
}
