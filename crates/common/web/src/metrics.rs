//! The metrics measured where requests are served, recorded on the global
//! meter provider `server::telemetry` installs (a no-op one when export is off).
//! An instrument records into the provider that was global when it was built,
//! so each is built by what records into it, once telemetry is set up, and
//! never held in a `static`.

use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::{Counter, Histogram, Meter};
use poem::http::{Method, StatusCode};
use std::fmt;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::{FilterFn, Filtered, LevelFilter, filter_fn};
use tracing_subscriber::layer::Context;

/// OpenTelemetry's advised boundaries for `http.server.request.duration`, in seconds.
const REQUEST_SECONDS: [f64; 14] = [
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0,
];
/// The acquire waits worth telling apart: from instant to the pool's 30 s timeout.
const WAIT_SECONDS: [f64; 12] = [
    0.0001, 0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 1.0, 5.0, 30.0,
];
/// The event sqlx emits as each connection is acquired, when the pool's
/// `acquire_time_level` (or, past its slow threshold, `acquire_slow_level`) is on.
const ACQUIRE_TARGET: &str = "sqlx::pool::acquire";

fn meter() -> Meter {
    global::meter("app-web")
}

/// `http.server.request.duration`, by operation, method, route and status.
#[derive(Clone)]
pub(crate) struct RequestDuration(Histogram<f64>);

impl RequestDuration {
    pub(crate) fn new() -> Self {
        Self(
            meter()
                .f64_histogram("http.server.request.duration")
                .with_unit("s")
                .with_description("How long the server took to answer each HTTP request.")
                .with_boundaries(REQUEST_SECONDS.to_vec())
                .build(),
        )
    }

    pub(crate) fn record(
        &self,
        seconds: f64,
        method: &Method,
        status: StatusCode,
        route: Option<&str>,
        operation: Option<&str>,
    ) {
        let mut attributes = vec![
            KeyValue::new("http.request.method", known(method)),
            KeyValue::new("http.response.status_code", i64::from(status.as_u16())),
        ];
        if let Some(route) = route {
            attributes.push(KeyValue::new("http.route", route.to_owned()));
        }
        if let Some(operation) = operation {
            attributes.push(KeyValue::new("operation_id", operation.to_owned()));
        }
        if status.is_server_error() {
            attributes.push(KeyValue::new("error.type", status.as_str().to_owned()));
        }
        self.0.record(seconds, &attributes);
    }
}

/// Semantic conventions: any method but the standard ones is `_OTHER`, so a
/// client can't mint attribute values.
fn known(method: &Method) -> &'static str {
    match *method {
        Method::CONNECT => "CONNECT",
        Method::DELETE => "DELETE",
        Method::GET => "GET",
        Method::HEAD => "HEAD",
        Method::OPTIONS => "OPTIONS",
        Method::PATCH => "PATCH",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::TRACE => "TRACE",
        _ => "_OTHER",
    }
}

/// `mcp.server.tool.calls`, by tool (`gen_ai.tool.name`) and `outcome`:
/// `ok`, or `error` for a call answered `isError` or with arguments that
/// make no request.
pub(crate) struct ToolCalls(Counter<u64>);

impl ToolCalls {
    pub(crate) fn new() -> Self {
        Self(
            meter()
                .u64_counter("mcp.server.tool.calls")
                .with_unit("{call}")
                .with_description("MCP tool calls, by tool and outcome.")
                .build(),
        )
    }

    pub(crate) fn add(&self, tool: &'static str, failed: bool) {
        self.0.add(
            1,
            &[
                KeyValue::new("gen_ai.tool.name", tool),
                KeyValue::new("outcome", if failed { "error" } else { "ok" }),
            ],
        );
    }
}

/// `idempotency.replays` and `idempotency.reuse_refusals`.
#[derive(Clone)]
pub(crate) struct IdempotencyOutcomes {
    replays: Counter<u64>,
    reuse_refusals: Counter<u64>,
}

impl IdempotencyOutcomes {
    pub(crate) fn new() -> Self {
        let meter = meter();
        Self {
            replays: meter
                .u64_counter("idempotency.replays")
                .with_unit("{response}")
                .with_description("Kept answers replayed to a retry, which didn't run again.")
                .build(),
            reuse_refusals: meter
                .u64_counter("idempotency.reuse_refusals")
                .with_unit("{request}")
                .with_description("Requests refused for reusing a key with another request.")
                .build(),
        }
    }

    pub(crate) fn replayed(&self) {
        self.replays.add(1, &[]);
    }

    pub(crate) fn reuse_refused(&self) {
        self.reuse_refusals.add(1, &[]);
    }
}

/// `db.client.connection.wait_time`: how long each connection took to
/// acquire from the pool, from the `sqlx::pool::acquire` event, which sqlx
/// emits only for a pool built with an `acquire_time_level` (`app_db::connect`'s).
/// sqlx offers no other hook around an acquire.
pub struct PoolWaits {
    wait_time: Histogram<f64>,
}

impl PoolWaits {
    /// Records on `meter`, which a subscriber installed before the global
    /// provider is set must be given. Filtered to sqlx's acquire events, so
    /// they reach it whatever the log level.
    pub fn layer<S: Subscriber>(meter: &Meter) -> Filtered<Self, FilterFn, S> {
        Self {
            wait_time: meter
                .f64_histogram("db.client.connection.wait_time")
                .with_unit("s")
                .with_description("How long acquiring a connection from the pool waited.")
                .with_boundaries(WAIT_SECONDS.to_vec())
                .build(),
        }
        .with_filter(
            filter_fn(is_acquire as fn(&Metadata<'_>) -> bool)
                .with_max_level_hint(LevelFilter::DEBUG),
        )
    }
}

impl<S: Subscriber> Layer<S> for PoolWaits {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut waited = Waited(None);
        event.record(&mut waited);
        if let Some(seconds) = waited.0 {
            self.wait_time.record(
                seconds,
                &[KeyValue::new("db.client.connection.pool.name", POOL_NAME)],
            );
        }
    }
}

/// The one pool each process holds, as `app_db::observe_pool` names it.
const POOL_NAME: &str = "app";

fn is_acquire(meta: &Metadata<'_>) -> bool {
    meta.target() == ACQUIRE_TARGET && *meta.level() <= Level::DEBUG
}

struct Waited(Option<f64>);

impl Visit for Waited {
    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "acquired_after_secs" {
            self.0 = Some(value);
        }
    }

    fn record_debug(&mut self, _: &Field, _: &dyn fmt::Debug) {}
}
