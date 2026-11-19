//! W3C trace context across the outbox (Spec 0003): a publisher records the
//! current span's in the envelope, as CloudEvents' distributed-tracing
//! extension, and the worker's span for each run of a job continues it.

use opentelemetry::propagation::{Extractor, Injector, TextMapPropagator};
use opentelemetry::{Context, trace::TraceContextExt};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use serde_json::Value;
use std::collections::HashMap;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

const TRACEPARENT: &str = "traceparent";
const TRACESTATE: &str = "tracestate";

/// The current span's `traceparent` and `tracestate`; neither outside any
/// trace (no OpenTelemetry layer, or no span entered).
pub(crate) fn current() -> (Option<String>, Option<String>) {
    let context = Span::current().context();
    if !context.span().span_context().is_valid() {
        return (None, None);
    }
    let mut fields = Fields::default();
    TraceContextPropagator::new().inject_context(&context, &mut fields);
    let tracestate = fields.0.remove(TRACESTATE).filter(|s| !s.is_empty());
    (fields.0.remove(TRACEPARENT), tracestate)
}

/// Makes `span` a child of the span that published `envelope`, or the root of
/// a new trace when the envelope names none (one published outside any trace,
/// or before envelopes carried one).
pub(crate) fn continue_published(span: &Span, envelope: &Value) {
    let publisher = TraceContextPropagator::new().extract(&Envelope(envelope));
    let parent = if publisher.span().span_context().is_valid() {
        publisher
    } else {
        Context::new()
    };
    // Fails only with no OpenTelemetry layer, when there is no trace to continue.
    let _ = span.set_parent(parent);
}

#[derive(Default)]
struct Fields(HashMap<String, String>);

impl Injector for Fields {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
}

struct Envelope<'a>(&'a Value);

impl Extractor for Envelope<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str)
    }

    fn keys(&self) -> Vec<&str> {
        vec![TRACEPARENT, TRACESTATE]
    }
}
