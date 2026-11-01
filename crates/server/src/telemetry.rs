use crate::config::Environment;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// A second call is a no-op (`try_init`).
pub fn init(env: Environment) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));

    let registry = tracing_subscriber::registry().with(filter);

    match env {
        Environment::Prod => {
            let _ = registry
                .with(fmt::layer().json().with_current_span(true))
                .try_init();
        }
        Environment::Dev | Environment::Test => {
            let _ = registry.with(fmt::layer().pretty()).try_init();
        }
    }

    // OpenTelemetry seam (spec §12, non-goal per §1): add `tracing-opentelemetry`
    // and layer `tracing_opentelemetry::layer().with_tracer(tracer)` above.
}
