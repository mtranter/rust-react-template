//! Logs to stdout and, when `OTEL_EXPORTER_OTLP_ENDPOINT` is set, traces and
//! metrics over OTLP, configured only by the standard `OTEL_*` variables (`.env.example`).

use crate::config::Environment;
use crate::{API_VERSION, APP_NAME};
use async_trait::async_trait;
use opentelemetry::metrics::MeterProvider;
use opentelemetry::trace::{TraceContextExt, TracerProvider};
use opentelemetry::{KeyValue, Value, global};
use opentelemetry_http::{Bytes, HttpClient, HttpError};
use opentelemetry_otlp::{
    MetricExporter, Protocol, SpanExporter, WithExportConfig, WithHttpConfig,
};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use poem::http::{Request, Response};
use std::collections::HashMap;
use std::env;
use std::fmt as std_fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::{Event, Metadata, Subscriber};
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Layer, fmt, prelude::*};

/// The OTLP HTTP client's own timeout: the exporter's default (`OTEL_EXPORTER_OTLP_TIMEOUT`).
const EXPORT_TIMEOUT: Duration = Duration::from_secs(10);
const WARN_AT_MOST_EVERY: Duration = Duration::from_secs(60);

/// The process's telemetry pipeline. Dropping it shuts the pipeline down,
/// exporting what is buffered; [`Telemetry::flush`] exports it and keeps going,
/// as a host that freezes the process between invocations needs.
#[must_use = "dropping it shuts the trace and metric pipelines down"]
pub struct Telemetry {
    tracer: Option<SdkTracerProvider>,
    meter: Option<SdkMeterProvider>,
}

impl Telemetry {
    /// Exports every finished span still buffered, and the metrics as they
    /// stand, blocking until they are sent.
    pub fn flush(&self) {
        if let Some(tracer) = &self.tracer
            && let Err(err) = tracer.force_flush()
        {
            tracing::warn!(error = %err, "flushing spans failed");
        }
        if let Some(meter) = &self.meter
            && let Err(err) = meter.force_flush()
        {
            tracing::warn!(error = %err, "flushing metrics failed");
        }
    }

    /// Exports what is buffered and stops exporting. Dropping it does the same.
    pub fn shutdown(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        if let Some(tracer) = self.tracer.take()
            && let Err(err) = tracer.shutdown()
        {
            tracing::warn!(error = %err, "shutting the trace pipeline down failed");
        }
        if let Some(meter) = self.meter.take()
            && let Err(err) = meter.shutdown()
        {
            tracing::warn!(error = %err, "shutting the metric pipeline down failed");
        }
    }
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        self.shut_down();
    }
}

/// `service` names the binary, `OTEL_SERVICE_NAME`'s default. A second call
/// installs nothing (`try_init`) and exports nothing. Call it before building
/// the pool and the app: their instruments record into the meter provider
/// installed when they are built.
pub fn init(env: Environment, service: &'static str) -> Telemetry {
    let (tracer, meter, problem) = match tracer(service).and_then(|tracer| {
        let meter = meter(service)?;
        Ok((tracer, meter))
    }) {
        Ok((tracer, meter)) => (tracer, meter, None),
        Err(problem) => (None, None, Some(problem)),
    };
    // Each layer filters by `RUST_LOG` on its own, not the registry, so that
    // sqlx's query events reach `QuerySpans` at `DEBUG` without being logged.
    let traces = tracer.as_ref().map(|provider| {
        tracing_opentelemetry::layer()
            .with_tracer(provider.tracer(service))
            .with_filter(levels())
            .with_filter(filter_fn(|meta| !is_export_path(meta)))
    });
    let queries = tracer
        .as_ref()
        .map(|provider| app_web::QuerySpans::layer(provider.tracer(service)));
    // Only with export on: sqlx then builds an event for each acquire.
    let pool_waits = meter
        .as_ref()
        .map(|provider| app_web::PoolWaits::layer(&provider.meter("app-web")));
    let registry = tracing_subscriber::registry()
        .with(Throttled::default())
        .with(traces)
        .with(queries)
        .with(pool_waits);

    let installed = match env {
        Environment::Prod => registry
            .with(
                fmt::layer()
                    .json()
                    .with_current_span(true)
                    .map_event_format(WithTraceIds::json)
                    .with_filter(levels()),
            )
            .try_init(),
        Environment::Dev | Environment::Test => registry
            .with(
                fmt::layer()
                    .pretty()
                    .map_event_format(WithTraceIds::pretty)
                    .with_filter(levels()),
            )
            .try_init(),
    };
    if let Some(problem) = problem {
        tracing::warn!(error = %problem, "exporting no traces or metrics");
    }
    if installed.is_err() {
        return Telemetry {
            tracer: None,
            meter: None,
        };
    }
    if let Some(meter) = &meter {
        global::set_meter_provider(meter.clone());
    }
    Telemetry { tracer, meter }
}

fn levels() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"))
}

fn exporting() -> bool {
    env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok_and(|endpoint| !endpoint.trim().is_empty())
}

/// `None` when `OTEL_EXPORTER_OTLP_ENDPOINT` is unset.
fn tracer(service: &'static str) -> Result<Option<SdkTracerProvider>, String> {
    if !exporting() {
        return Ok(None);
    }
    let exporter = SpanExporter::builder()
        .with_http()
        .with_protocol(protocol()?)
        .with_http_client(Blocking::new()?)
        .build()
        .map_err(|err| err.to_string())?;
    // The batch processor exports from a thread of its own, so a slow or
    // unreachable collector fills its queue, and drops spans, rather than
    // holding up a request. The sampler is read from `OTEL_TRACES_SAMPLER`.
    Ok(Some(
        SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource(service))
            .build(),
    ))
}

/// `None` when `OTEL_EXPORTER_OTLP_ENDPOINT` is unset.
fn meter(service: &'static str) -> Result<Option<SdkMeterProvider>, String> {
    if !exporting() {
        return Ok(None);
    }
    let exporter = MetricExporter::builder()
        .with_http()
        .with_protocol(protocol()?)
        .with_http_client(Blocking::new()?)
        .build()
        .map_err(|err| err.to_string())?;
    // A request only adds to aggregates in memory; the periodic reader
    // collects and exports them from a thread of its own, every
    // `OTEL_METRIC_EXPORT_INTERVAL` (60 s by default).
    Ok(Some(
        SdkMeterProvider::builder()
            .with_periodic_exporter(exporter)
            .with_resource(resource(service))
            .build(),
    ))
}

/// `OTEL_EXPORTER_OTLP_PROTOCOL`, `http/protobuf` by default. gRPC isn't built
/// in: it would bring tonic and its TLS stack into the one binary for a choice
/// every collector also serves over HTTP.
fn protocol() -> Result<Protocol, String> {
    match env::var("OTEL_EXPORTER_OTLP_PROTOCOL")
        .as_deref()
        .map(str::trim)
    {
        Err(_) | Ok("") | Ok("http/protobuf") => Ok(Protocol::HttpBinary),
        Ok("http/json") => Ok(Protocol::HttpJson),
        Ok(other) => Err(format!(
            "OTEL_EXPORTER_OTLP_PROTOCOL={other} isn't supported: use http/protobuf or http/json"
        )),
    }
}

/// From `OTEL_SERVICE_NAME` and `OTEL_RESOURCE_ATTRIBUTES` (read by the
/// SDK's detectors), else `service`; with the app's name and version.
fn resource(service: &'static str) -> Resource {
    let detected = Resource::builder().build();
    let named = detected
        .get(&"service.name".into())
        .is_some_and(|name| !name.as_str().starts_with("unknown_service"));
    let builder = Resource::builder().with_attributes([
        KeyValue::new("service.namespace", APP_NAME),
        KeyValue::new("service.version", API_VERSION),
    ]);
    if named {
        builder.build()
    } else {
        builder.with_service_name(Value::from(service)).build()
    }
}

/// The spans and events the exporter's own HTTP calls make, which would
/// otherwise each become a span to export.
fn is_export_path(meta: &Metadata<'_>) -> bool {
    ["opentelemetry", "hyper", "h2", "reqwest"]
        .iter()
        .any(|prefix| meta.target().starts_with(prefix))
}

/// The OTLP client: reqwest 0.12, the workspace's, without its own runtime.
/// The batch processor's thread blocks on it. opentelemetry-otlp's own
/// reqwest feature would bring reqwest 0.13 and its aws-lc-rs (ADR 0005).
#[derive(Debug)]
struct Blocking(reqwest::blocking::Client);

impl Blocking {
    fn new() -> Result<Self, String> {
        // A blocking client may not be built inside an async runtime.
        std::thread::spawn(|| {
            reqwest::blocking::Client::builder()
                .timeout(EXPORT_TIMEOUT)
                .build()
        })
        .join()
        .map_err(|_| "building the OTLP HTTP client panicked".to_string())?
        .map(Self)
        .map_err(|err| err.to_string())
    }
}

#[async_trait]
impl HttpClient for Blocking {
    async fn send_bytes(&self, request: Request<Bytes>) -> Result<Response<Bytes>, HttpError> {
        let (parts, body) = request.into_parts();
        let sent = self
            .0
            .request(parts.method, parts.uri.to_string())
            .headers(parts.headers)
            .body(body.to_vec())
            .send()?;
        let mut response = Response::builder().status(sent.status());
        if let Some(headers) = response.headers_mut() {
            *headers = sent.headers().clone();
        }
        Ok(response.body(sent.bytes()?)?)
    }
}

/// Lets each of the OpenTelemetry SDK's own warnings and errors (a span
/// dropped from a full queue, a failed export) through at most once a minute.
#[derive(Default)]
struct Throttled {
    last: Mutex<HashMap<&'static str, Instant>>,
}

impl Throttled {
    fn allows(&self, name: &'static str, now: Instant) -> bool {
        let mut last = self
            .last
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match last.get(name) {
            Some(at) if now.duration_since(*at) < WARN_AT_MOST_EVERY => false,
            _ => {
                last.insert(name, now);
                true
            }
        }
    }
}

impl<S: Subscriber> Layer<S> for Throttled {
    fn event_enabled(&self, event: &Event<'_>, _: Context<'_, S>) -> bool {
        let meta = event.metadata();
        !meta.target().starts_with("opentelemetry") || self.allows(meta.name(), Instant::now())
    }
}

/// Adds the current span's `trace_id` and `span_id` to a log line, when
/// there is one: as fields of the JSON object, or a last line in the pretty format.
struct WithTraceIds<F> {
    inner: F,
    json: bool,
}

impl<F> WithTraceIds<F> {
    fn json(inner: F) -> Self {
        Self { inner, json: true }
    }

    fn pretty(inner: F) -> Self {
        Self { inner, json: false }
    }
}

impl<S, N, F> FormatEvent<S, N> for WithTraceIds<F>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
    F: FormatEvent<S, N>,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std_fmt::Result {
        // tracing-opentelemetry makes an entered span's context current.
        let context = opentelemetry::Context::current();
        let span = context.span();
        let ids = span.span_context();
        if !ids.is_valid() {
            return self.inner.format_event(ctx, writer, event);
        }
        let (trace_id, span_id) = (ids.trace_id(), ids.span_id());
        if self.json {
            let mut line = String::new();
            self.inner
                .format_event(ctx, Writer::new(&mut line), event)?;
            let fields = format!(r#"{{"trace_id":"{trace_id}","span_id":"{span_id}","#);
            match line.strip_prefix('{') {
                Some(rest) => write!(writer, "{fields}{rest}"),
                None => writer.write_str(&line),
            }
        } else {
            self.inner.format_event(ctx, writer.by_ref(), event)?;
            writeln!(writer, "    trace_id: {trace_id}, span_id: {span_id}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// What one event logs, in a span and out of one, with tracing on.
    fn logged(json: bool) -> (String, String, String) {
        let provider = SdkTracerProvider::builder().build();
        let captured = Captured::default();
        let writer = {
            let captured = captured.clone();
            move || captured.clone()
        };
        let traces = tracing_opentelemetry::layer().with_tracer(provider.tracer("test"));
        let subscriber = tracing_subscriber::registry().with(traces);
        let ids = if json {
            let subscriber = subscriber.with(
                fmt::layer()
                    .json()
                    .with_writer(writer)
                    .map_event_format(WithTraceIds::json),
            );
            tracing::subscriber::with_default(subscriber, log_in_and_out_of_a_span)
        } else {
            let subscriber = subscriber.with(
                fmt::layer()
                    .pretty()
                    .with_ansi(false)
                    .with_writer(writer)
                    .map_event_format(WithTraceIds::pretty),
            );
            tracing::subscriber::with_default(subscriber, log_in_and_out_of_a_span)
        };
        let lines = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        (lines, ids.0, ids.1)
    }

    fn log_in_and_out_of_a_span() -> (String, String) {
        tracing::info!("outside");
        let span = tracing::info_span!("work");
        let _entered = span.enter();
        tracing::info!("inside");
        let context = opentelemetry::Context::current();
        let ids = context.span().span_context().clone();
        (ids.trace_id().to_string(), ids.span_id().to_string())
    }

    #[test]
    fn a_json_line_in_a_span_carries_its_trace_and_span_ids() {
        let (lines, trace_id, span_id) = logged(true);
        let lines: Vec<serde_json::Value> = lines
            .lines()
            .map(|line| serde_json::from_str(line).expect("each line is JSON"))
            .collect();
        assert_eq!(lines[0].get("trace_id"), None);
        assert_eq!(lines[1]["trace_id"], trace_id.as_str());
        assert_eq!(lines[1]["span_id"], span_id.as_str());
        assert_eq!(lines[1]["fields"]["message"], "inside");
    }

    #[test]
    fn a_pretty_line_in_a_span_carries_its_trace_and_span_ids() {
        let (lines, trace_id, span_id) = logged(false);
        let (outside, inside) = lines.split_once("inside").expect("both events are logged");
        assert!(!outside.contains("trace_id"));
        assert!(inside.contains(&format!("trace_id: {trace_id}, span_id: {span_id}")));
    }

    #[test]
    fn a_warning_repeats_only_after_a_minute() {
        let throttled = Throttled::default();
        let start = Instant::now();
        assert!(throttled.allows("ExportError", start));
        assert!(!throttled.allows("ExportError", start + Duration::from_secs(59)));
        assert!(throttled.allows("SpanDroppingStarted", start + Duration::from_secs(59)));
        assert!(throttled.allows("ExportError", start + Duration::from_secs(60)));
    }
}
