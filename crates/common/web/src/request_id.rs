//! Request-id / correlation-id middleware.
//!
//! Honours an inbound `x-request-id` (e.g. from a load balancer) or mints a new
//! UUID, attaches it to a tracing span so every log line for the request carries
//! it, and echoes it back on the response.

use poem::{
    Endpoint, IntoResponse, Middleware, Request, Response, Result,
    http::{HeaderValue, header::HeaderName},
};
use tracing::Instrument;
use uuid::Uuid;

const HEADER: &str = "x-request-id";

/// The request id, available to handlers via `Data<RequestId>` is not used here;
/// the value lives in request extensions and the tracing span.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Middleware that assigns a correlation id to every request.
pub struct RequestIdMiddleware;

impl<E: Endpoint> Middleware<E> for RequestIdMiddleware {
    type Output = RequestIdEndpoint<E>;

    fn transform(&self, inner: E) -> Self::Output {
        RequestIdEndpoint { inner }
    }
}

pub struct RequestIdEndpoint<E> {
    inner: E,
}

impl<E: Endpoint> Endpoint for RequestIdEndpoint<E> {
    type Output = Response;

    async fn call(&self, mut req: Request) -> Result<Self::Output> {
        let id = req
            .headers()
            .get(HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        req.extensions_mut().insert(RequestId(id.clone()));

        let span = tracing::info_span!("http_request", request_id = %id);
        let mut resp = self.inner.call(req).instrument(span).await?.into_response();

        if let Ok(value) = HeaderValue::from_str(&id) {
            resp.headers_mut()
                .insert(HeaderName::from_static(HEADER), value);
        }
        Ok(resp)
    }
}
