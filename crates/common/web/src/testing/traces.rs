//! An in-memory span exporter, for a test to check the spans its requests make.

use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData};
use std::sync::Once;
use tracing::subscriber::{DefaultGuard, Interest};
use tracing::{Metadata, span};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;

/// Every span finished on this thread while it is held, as the server's
/// pipeline would export it. It is installed for the current thread only, so
/// tests running in parallel keep their spans apart: `#[tokio::test]`'s default
/// runtime runs the test, and the app it drives, on one thread.
pub struct Traces {
    exporter: InMemorySpanExporter,
    _provider: SdkTracerProvider,
    _installed: DefaultGuard,
}

impl Traces {
    pub fn install() -> Self {
        static UNDECIDED: Once = Once::new();
        UNDECIDED.call_once(|| {
            // Fails only where a test already set its own global subscriber.
            let _ = tracing::subscriber::set_global_default(Undecided);
        });
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
        Self {
            exporter,
            _provider: provider,
            _installed: tracing::subscriber::set_default(subscriber),
        }
    }

    pub fn spans(&self) -> Vec<SpanData> {
        self.exporter
            .get_finished_spans()
            .expect("the in-memory exporter is never shut down while held")
    }

    /// Panics unless exactly one span has finished.
    pub fn only_span(&self) -> SpanData {
        let mut spans = self.spans();
        assert_eq!(
            spans.len(),
            1,
            "expected one span, got {:?}",
            spans.iter().map(|span| &span.name).collect::<Vec<_>>()
        );
        spans.remove(0)
    }

    /// The finished span named `name`; panics unless there is exactly one.
    pub fn span(&self, name: &str) -> SpanData {
        let spans = self.spans();
        let names: Vec<_> = spans.iter().map(|span| span.name.to_string()).collect();
        let mut named: Vec<_> = spans.into_iter().filter(|span| span.name == name).collect();
        assert_eq!(
            named.len(),
            1,
            "expected one span named {name:?} in {names:?}"
        );
        named.remove(0)
    }

    /// `span`'s attribute `key`, as a string.
    pub fn attribute(span: &SpanData, key: &str) -> Option<String> {
        span.attributes
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| kv.value.to_string())
    }
}

/// tracing-core caches each callsite's interest, and the most verbose level
/// any subscriber wants, across every thread, rebuilding them as per-thread
/// subscribers come and go; two threads rebuilding at once can leave a span
/// disabled for a thread whose subscriber wants it. A global subscriber that
/// wants every level and decides per call keeps both open, so each span asks
/// its thread's subscriber.
struct Undecided;

impl tracing::Subscriber for Undecided {
    fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
        Interest::sometimes()
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::TRACE)
    }

    fn enabled(&self, _: &Metadata<'_>) -> bool {
        false
    }

    fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }

    fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

    fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

    fn event(&self, _: &tracing::Event<'_>) {}

    fn enter(&self, _: &span::Id) {}

    fn exit(&self, _: &span::Id) {}
}
