//! Honours an inbound `x-request-id` (e.g. from a load balancer) before minting one.
//! Put it outermost, so every response carries the header (errors included) and
//! every log line inside, `Tracing`'s access log too, carries the id.

use poem::{
    Endpoint, IntoResponse, Middleware, Request, Response, Result,
    http::{HeaderValue, header::HeaderName},
};
use tracing::Instrument;
use uuid::Uuid;

const HEADER: &str = "x-request-id";

tokio::task_local! {
    // Lets an `ErrorBody` built anywhere in the request echo the id.
    static CURRENT: String;
}

/// `None` outside a request wrapped by [`RequestIdMiddleware`].
pub(crate) fn current_request_id() -> Option<String> {
    CURRENT.try_with(Clone::clone).ok()
}

#[derive(Debug, Clone)]
pub struct RequestId(pub String);

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
        let result = CURRENT
            .scope(id.clone(), self.inner.call(req))
            .instrument(span)
            .await;
        let mut resp = match result {
            Ok(resp) => resp.into_response(),
            Err(err) => err.into_response(),
        };

        if let Ok(value) = HeaderValue::from_str(&id) {
            resp.headers_mut()
                .insert(HeaderName::from_static(HEADER), value);
        }
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ApiError, render_error};
    use app_error::AppError;
    use poem::{EndpointExt, Route, get, handler, test::TestClient};

    #[handler]
    fn refuse() -> Result<()> {
        Err(ApiError::from(AppError::invalid_field("title", "too long")).into())
    }

    fn app() -> TestClient<impl Endpoint> {
        let app = Route::new()
            .at("/refuse", get(refuse))
            .catch_all_error(render_error)
            .with(RequestIdMiddleware);
        TestClient::new(app)
    }

    async fn assert_id_in_header_and_body(resp: poem::test::TestResponse) -> String {
        let header = resp
            .0
            .headers()
            .get(HEADER)
            .expect("the response carries the request id")
            .to_str()
            .unwrap()
            .to_owned();
        let body = resp.json().await;
        body.value()
            .object()
            .get("requestId")
            .assert_string(&header);
        header
    }

    #[tokio::test]
    async fn a_refusal_carries_the_request_id_in_header_and_body() {
        let resp = app().get("/refuse").send().await;
        resp.assert_status(poem::http::StatusCode::BAD_REQUEST);
        assert_id_in_header_and_body(resp).await;
    }

    #[tokio::test]
    async fn a_framework_error_carries_the_request_id_in_header_and_body() {
        let resp = app().get("/nowhere").send().await;
        resp.assert_status(poem::http::StatusCode::NOT_FOUND);
        assert_id_in_header_and_body(resp).await;
    }

    #[tokio::test]
    async fn an_inbound_request_id_is_honoured() {
        let resp = app()
            .get("/refuse")
            .header(HEADER, "from-the-load-balancer")
            .send()
            .await;
        assert_eq!(
            assert_id_in_header_and_body(resp).await,
            "from-the-load-balancer"
        );
    }

    #[tokio::test]
    async fn an_error_not_caught_upstream_still_gets_the_header() {
        let app = Route::new().with(RequestIdMiddleware);
        let resp = TestClient::new(app).get("/nowhere").send().await;
        resp.assert_status(poem::http::StatusCode::NOT_FOUND);
        resp.assert_header_exist(HEADER);
    }
}
