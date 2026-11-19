//! One span per HTTP request, continuing the caller's trace from a W3C
//! `traceparent` header. Put it inside [`RequestIdMiddleware`](crate::RequestIdMiddleware),
//! whose id it carries, and outside the error rendering, whose status it records.
//! It also writes the access log line, in the span. With no OpenTelemetry layer
//! installed (`OTEL_EXPORTER_OTLP_ENDPOINT` unset) it is only a `tracing` span.

use crate::RequestId;
use opentelemetry::propagation::{Extractor, TextMapPropagator};
use opentelemetry::trace::TraceContextExt;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use poem::http::HeaderMap;
use poem::{Endpoint, IntoResponse, Middleware, PathPattern, Request, Response, Result};
use poem_openapi::{OpenApi, OperationId};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::field::Empty;
use tracing::{Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

#[derive(Clone, Default)]
pub struct TraceRequests {
    routes: Arc<HashMap<&'static str, String>>,
}

impl TraceRequests {
    /// Names each of `T`'s operations' spans by its route as the spec writes
    /// it (`/api/notes/{id}`), served under `base`. poem's own route for an
    /// operation renames its parameters (`/api/notes/:param0`).
    pub fn of<T: OpenApi>(base: &str) -> Self {
        let routes = T::meta()
            .into_iter()
            .flat_map(|api| api.paths)
            .flat_map(|path| {
                let route = format!("{base}{}", path.path);
                path.operations
                    .into_iter()
                    .filter_map(move |op| Some((op.operation_id?, route.clone())))
            })
            .collect();
        Self {
            routes: Arc::new(routes),
        }
    }
}

impl<E: Endpoint> Middleware<E> for TraceRequests {
    type Output = TraceRequestsEndpoint<E>;

    fn transform(&self, inner: E) -> Self::Output {
        TraceRequestsEndpoint {
            inner,
            routes: self.routes.clone(),
        }
    }
}

pub struct TraceRequestsEndpoint<E> {
    inner: E,
    routes: Arc<HashMap<&'static str, String>>,
}

/// The request's span, in request data, so what is learned inside the request
/// (the subject) is recorded on it rather than on whatever span is current.
#[derive(Clone)]
struct RequestSpan(Span);

/// Records the principal's subject on the request's span.
pub(crate) fn record_subject(req: &Request, subject: &str) {
    if let Some(RequestSpan(span)) = req.data::<RequestSpan>() {
        span.record("user.id", subject);
    }
}

impl<E: Endpoint> Endpoint for TraceRequestsEndpoint<E> {
    type Output = Response;

    async fn call(&self, mut req: Request) -> Result<Self::Output> {
        let method = req.method().clone();
        // Field names follow OpenTelemetry's HTTP semantic conventions; `otel.*`
        // are tracing-opentelemetry's, setting the span's kind and status.
        let span = tracing::info_span!(
            "request",
            otel.kind = "server",
            otel.status_code = Empty,
            http.request.method = %method,
            url.path = req.uri().path(),
            http.route = Empty,
            http.response.status_code = Empty,
            operation_id = Empty,
            request_id = Empty,
            user.id = Empty,
        );
        if let Some(RequestId(id)) = req.extensions().get::<RequestId>() {
            span.record("request_id", id.as_str());
        }
        let caller = TraceContextPropagator::new().extract(&Headers(req.headers()));
        if caller.span().span_context().is_valid() {
            // Fails only with no OpenTelemetry layer, when there is no trace to continue.
            let _ = span.set_parent(caller);
        }
        req.set_data(RequestSpan(span.clone()));

        let started = Instant::now();
        let resp = match self.inner.call(req).instrument(span.clone()).await {
            Ok(resp) => resp.into_response(),
            Err(err) => err.into_response(),
        };

        let status = resp.status();
        let operation = resp.data::<OperationId>().map(|OperationId(id)| *id);
        let route = operation
            .and_then(|id| self.routes.get(id))
            .map(String::as_str)
            .or_else(|| {
                resp.data::<PathPattern>()
                    .map(|PathPattern(route)| &**route)
            });
        // Named here, not by `otel.name`, which tracing-opentelemetry ignores once the span has started.
        let name = match route {
            Some(route) => {
                span.record("http.route", route);
                format!("{method} {route}")
            }
            None => method.to_string(),
        };
        span.context().span().update_name(name);
        if let Some(id) = operation {
            span.record("operation_id", id);
        }
        span.record("http.response.status_code", status.as_u16());
        if status.is_server_error() {
            span.record("otel.status_code", "error");
        }
        span.in_scope(|| {
            tracing::info!(status = %status, duration = ?started.elapsed(), "response");
        });
        Ok(resp)
    }
}

struct Headers<'a>(&'a HeaderMap);

impl Extractor for Headers<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|name| name.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Traces;
    use crate::{RequestIdMiddleware, render_error};
    use opentelemetry::trace::{SpanKind, Status};
    use poem::{EndpointExt, Route, get, handler, test::TestClient};

    #[handler]
    fn ok() -> &'static str {
        "ok"
    }

    #[handler]
    fn fail() -> Result<()> {
        Err(poem::Error::from_status(
            poem::http::StatusCode::INTERNAL_SERVER_ERROR,
        ))
    }

    fn app() -> TestClient<impl Endpoint> {
        let app = Route::new()
            .at("/things/:id", get(ok))
            .at("/fail", get(fail))
            .catch_all_error(render_error)
            .with(TraceRequests::default())
            .with(RequestIdMiddleware);
        TestClient::new(app)
    }

    #[tokio::test]
    async fn a_request_is_one_server_span_named_by_its_route() {
        let traces = Traces::install();
        app()
            .get("/things/42")
            .header("x-request-id", "req-1")
            .send()
            .await
            .assert_status_is_ok();

        let span = traces.only_span();
        assert_eq!(span.name, "GET /things/:id");
        assert_eq!(span.span_kind, SpanKind::Server);
        assert_eq!(
            Traces::attribute(&span, "http.route").unwrap(),
            "/things/:id"
        );
        assert_eq!(Traces::attribute(&span, "request_id").unwrap(), "req-1");
        assert_eq!(
            Traces::attribute(&span, "http.response.status_code").unwrap(),
            "200"
        );
    }

    #[tokio::test]
    async fn a_server_error_marks_the_span_an_error() {
        let traces = Traces::install();
        app().get("/fail").send().await;

        let span = traces.only_span();
        assert!(matches!(span.status, Status::Error { .. }));
        assert_eq!(
            Traces::attribute(&span, "http.response.status_code").unwrap(),
            "500"
        );
    }

    #[tokio::test]
    async fn an_unrouted_request_is_named_by_its_method() {
        let traces = Traces::install();
        app().get("/nowhere").send().await;

        assert_eq!(traces.only_span().name, "GET");
    }
}
