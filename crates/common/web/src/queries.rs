//! One span per SQL query run through sqlx, built from the `sqlx::query` event
//! sqlx emits as each query finishes. sqlx makes no span of its own and offers
//! no hook around a query, and the event is the one place every query passes,
//! on the pool, a connection or a transaction alike. It carries the statement
//! as written, with `$1` placeholders, and never the values bound to them.

use opentelemetry::trace::{Span as _, SpanKind, TraceContextExt, Tracer};
use opentelemetry::{Context as OtelContext, KeyValue};
use std::fmt;
use std::time::{Duration, SystemTime};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::{FilterFn, Filtered, LevelFilter, filter_fn};
use tracing_subscriber::layer::Context;

const TARGET: &str = "sqlx::query";

/// Install it beside `tracing_opentelemetry`'s layer, with the same tracer.
/// A query outside any span records nothing: an idle worker's polls would
/// otherwise each export a trace of their own.
pub struct QuerySpans<T> {
    tracer: T,
}

impl<T: Tracer + Send + Sync + 'static> QuerySpans<T> {
    /// Filtered to sqlx's query events, at the `DEBUG` level sqlx emits them
    /// at, so sqlx formats them only for this layer, whatever the log level.
    pub fn layer<S: Subscriber>(tracer: T) -> Filtered<Self, FilterFn, S> {
        Self { tracer }.with_filter(
            filter_fn(is_query as fn(&Metadata<'_>) -> bool)
                .with_max_level_hint(LevelFilter::DEBUG),
        )
    }
}

impl<S, T> Layer<S> for QuerySpans<T>
where
    S: Subscriber,
    T: Tracer + Send + Sync + 'static,
{
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        // tracing-opentelemetry makes the entered span's context current.
        let parent = OtelContext::current();
        if !parent.span().span_context().is_valid() {
            return;
        }
        let mut query = Query::default();
        event.record(&mut query);
        let end = SystemTime::now();
        let start = end.checked_sub(query.elapsed).unwrap_or(end);
        let Some(text) = query.text() else {
            return;
        };
        let name = text
            .split_whitespace()
            .next()
            .unwrap_or("query")
            .to_uppercase();
        let mut span = self
            .tracer
            .span_builder(name)
            .with_kind(SpanKind::Client)
            .with_start_time(start)
            .with_attributes([
                KeyValue::new("db.system.name", "postgresql"),
                KeyValue::new("db.query.text", text),
            ])
            .start_with_context(&self.tracer, &parent);
        span.end_with_timestamp(end);
    }
}

fn is_query(meta: &Metadata<'_>) -> bool {
    meta.target() == TARGET && *meta.level() <= Level::DEBUG
}

/// sqlx records a statement of up to four words as `summary` alone, and a
/// longer one in full as `db.statement`, with its first four words as `summary`.
#[derive(Default)]
struct Query {
    summary: Option<String>,
    statement: Option<String>,
    elapsed: Duration,
}

impl Query {
    fn text(self) -> Option<String> {
        self.statement
            .map(|statement| statement.trim().to_owned())
            .filter(|statement| !statement.is_empty())
            .or(self.summary)
    }
}

impl Visit for Query {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "summary" => self.summary = Some(value.to_owned()),
            "db.statement" => self.statement = Some(value.to_owned()),
            _ => {}
        }
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "elapsed_secs" {
            self.elapsed = Duration::try_from_secs_f64(value).unwrap_or_default();
        }
    }

    fn record_debug(&mut self, _: &Field, _: &dyn fmt::Debug) {}
}
