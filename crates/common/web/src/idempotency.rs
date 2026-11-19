//! `Idempotency-Key` on every unsafe method (POST, PUT, PATCH, DELETE): the
//! first request with a key runs, and its successful answer is kept for 24
//! hours and replayed to a retry with the same key, which doesn't run again. So
//! a client that lost a response can retry a `PATCH` or `DELETE` and get its
//! answer, rather than a spurious `412` or `404` from the change it already made.
//!
//! A claim left unanswered for a minute (its client gone, or the server crashed
//! mid-request) is taken to be abandoned: a retry of the same request claims the
//! key and runs. A request still running after a minute may so run twice; a
//! minute is well past any answer this API gives. Keys are the caller's (the
//! bearer token's subject, or one shared scope for callers without a valid
//! token), so one caller's key never replays another's answer.

use crate::auth::verified_caller;
use crate::error::{ApiError, ErrorBody, ErrorCode, render_error};
use crate::metrics::IdempotencyOutcomes;
use app_domain::Clock;
use app_error::{AppError, AppResult};
use app_persistence::{Claimed, IdempotencyRecords, KeyClaim, RecordedResponse};
use chrono::TimeDelta;
use poem::http::{HeaderName, HeaderValue, Method, StatusCode, header};
use poem::{Body, Endpoint, IntoResponse, Middleware, Request, Response, Result};
use poem_openapi::payload::Json;
use poem_openapi::registry::{MetaApi, MetaOperationParam, MetaParamIn, Registry};
use poem_openapi::types::Type;
use poem_openapi::{ApiResponse, OpenApi};
use std::collections::HashMap;
use std::sync::Arc;

pub const IDEMPOTENCY_KEY: &str = "Idempotency-Key";
const RETAINED: TimeDelta = TimeDelta::hours(24);
const IN_FLIGHT_LEASE: TimeDelta = TimeDelta::minutes(1);
const MAX_KEY_LENGTH: usize = 255;

/// Put it inside the request id, so its refusals carry one, and outside the
/// error rendering, so it keeps the answer a client got.
#[derive(Clone)]
pub struct Idempotency {
    records: Arc<dyn IdempotencyRecords>,
    clock: Arc<dyn Clock>,
    outcomes: IdempotencyOutcomes,
}

impl Idempotency {
    pub fn new(records: Arc<dyn IdempotencyRecords>, clock: Arc<dyn Clock>) -> Self {
        Self {
            records,
            clock,
            outcomes: IdempotencyOutcomes::new(),
        }
    }

    async fn claim(&self, caller: String, key: String, request: &[u8]) -> AppResult<Keyed<'_>> {
        if key.is_empty() || key.chars().count() > MAX_KEY_LENGTH {
            return Err(AppError::invalid_field(
                IDEMPOTENCY_KEY,
                format!("must be 1 to {MAX_KEY_LENGTH} characters"),
            ));
        }
        let now = self.clock.now();
        let claim = KeyClaim {
            caller,
            key,
            at: now,
        };
        Ok(
            match self
                .records
                .claim(&claim, request, now - RETAINED, now - IN_FLIGHT_LEASE)
                .await?
            {
                Claimed::Claimed => Keyed::Claimed(Claim {
                    records: &*self.records,
                    claim,
                }),
                Claimed::ForAnotherRequest => Keyed::ForAnotherRequest,
                Claimed::InFlight => Keyed::InFlight,
                Claimed::Answered(recorded) => Keyed::Answered(recorded),
            },
        )
    }
}

/// What a claimed key says to do with its request.
enum Keyed<'a> {
    /// Run it, then keep or release its claim.
    Claimed(Claim<'a>),
    ForAnotherRequest,
    InFlight,
    /// Answer it this way, without running it.
    Answered(RecordedResponse),
}

/// A key claimed for a request about to run.
struct Claim<'a> {
    records: &'a dyn IdempotencyRecords,
    claim: KeyClaim,
}

impl Claim<'_> {
    /// Keeps the request's successful `answer` for its retries. The request
    /// has run, so unkept it is answered all the same: its claim stays in
    /// flight until abandoned, and a retry after that runs it again.
    async fn keep(self, answer: &RecordedResponse) {
        if let Err(err) = self.records.complete(&self.claim, answer).await {
            tracing::warn!(error = %err, "could not keep an idempotent answer");
        }
    }

    /// Frees the key of a request refused or failed, so a retry runs again.
    async fn release(self) {
        if let Err(err) = self.records.release(&self.claim).await {
            tracing::warn!(error = %err, "could not release an idempotency key");
        }
    }
}

impl<E: Endpoint> Middleware<E> for Idempotency {
    type Output = IdempotencyEndpoint<E>;

    fn transform(&self, inner: E) -> Self::Output {
        IdempotencyEndpoint {
            inner,
            idempotency: self.clone(),
        }
    }
}

pub struct IdempotencyEndpoint<E> {
    inner: E,
    idempotency: Idempotency,
}

impl<E: Endpoint> Endpoint for IdempotencyEndpoint<E> {
    type Output = Response;

    async fn call(&self, req: Request) -> Result<Self::Output> {
        match self.answer(req).await {
            Ok(response) => Ok(response),
            // Outside the error rendering, so it renders its own.
            Err(err) => Ok(render_error(err).await),
        }
    }
}

impl<E: Endpoint> IdempotencyEndpoint<E> {
    async fn answer(&self, mut req: Request) -> Result<Response> {
        let Some(key) = key_of(&req) else {
            return self.run(req).await;
        };
        let caller = caller_of(&req).await;
        let body = req.take_body().into_bytes().await?;
        let request = fingerprint(&req, &body);
        req.set_body(body);

        let claim = match self.idempotency.claim(caller, key, &request).await {
            Ok(Keyed::Claimed(claim)) => claim,
            Ok(Keyed::ForAnotherRequest) => {
                self.idempotency.outcomes.reuse_refused();
                return Ok(IdempotencyRefusal::reused().into_response());
            }
            Ok(Keyed::InFlight) => return Ok(IdempotencyRefusal::in_flight().into_response()),
            Ok(Keyed::Answered(recorded)) => {
                self.idempotency.outcomes.replayed();
                return Ok(replay(recorded));
            }
            Err(err) => return Ok(ApiError::from(err).into_response()),
        };

        let answered = async {
            let response = self.run(req).await?;
            if !response.status().is_success() {
                return Ok(Err(response));
            }
            let (parts, body) = response.into_parts();
            Ok(Ok((parts, body.into_vec().await?)))
        };
        let (parts, body) = match answered.await {
            Ok(Ok(answered)) => answered,
            Ok(Err(refused)) => {
                claim.release().await;
                return Ok(refused);
            }
            Err(err) => {
                claim.release().await;
                return Err(err);
            }
        };
        claim
            .keep(&RecordedResponse {
                status: parts.status.as_u16(),
                headers: parts
                    .headers
                    .iter()
                    .filter_map(|(name, value)| {
                        Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned()))
                    })
                    .collect(),
                body: body.clone(),
            })
            .await;
        Ok(Response::from_parts(parts, Body::from(body)))
    }

    async fn run(&self, req: Request) -> Result<Response> {
        self.inner.call(req).await.map(IntoResponse::into_response)
    }
}

/// Only an unsafe method's key counts: a safe one changes nothing to repeat.
fn key_of(req: &Request) -> Option<String> {
    if !is_unsafe(req.method()) {
        return None;
    }
    let value = req.headers().get(IDEMPOTENCY_KEY)?;
    Some(String::from_utf8_lossy(value.as_bytes()).into_owned())
}

fn is_unsafe(method: &Method) -> bool {
    [Method::POST, Method::PUT, Method::PATCH, Method::DELETE].contains(method)
}

/// The subject of a valid bearer token, or empty. An invalid token falls into
/// the empty scope, where the operation refuses it if it needs one; a refusal
/// is never kept.
async fn caller_of(req: &Request) -> String {
    verified_caller(req)
        .await
        .map(|claims| claims.sub)
        .unwrap_or_default()
}

/// What makes two requests with one key the same request: where they went,
/// the version they expected and what they sent.
fn fingerprint(req: &Request, body: &[u8]) -> Vec<u8> {
    let target = req
        .uri()
        .path_and_query()
        .map_or_else(|| req.uri().path(), |target| target.as_str());
    let if_match = req
        .headers()
        .get(header::IF_MATCH)
        .map_or(&[][..], HeaderValue::as_bytes);
    [
        req.method().as_str().as_bytes(),
        b" ",
        target.as_bytes(),
        b"\n",
        if_match,
        b"\n",
        body,
    ]
    .concat()
}

fn replay(recorded: RecordedResponse) -> Response {
    let mut response = Response::builder()
        .status(StatusCode::from_u16(recorded.status).unwrap_or(StatusCode::OK))
        .body(recorded.body);
    let headers = response.headers_mut();
    for (name, value) in recorded.headers {
        if let (Ok(name), Ok(value)) = (HeaderName::try_from(name), HeaderValue::try_from(value)) {
            headers.append(name, value);
        }
    }
    response
}

#[derive(ApiResponse)]
enum IdempotencyRefusal {
    /// A request with this `Idempotency-Key` is still being answered: retry later.
    #[oai(status = 409)]
    InFlight(Json<ErrorBody>),
    /// This `Idempotency-Key` was sent with a different request.
    #[oai(status = 422)]
    Reused(Json<ErrorBody>),
}

impl IdempotencyRefusal {
    fn in_flight() -> Self {
        Self::InFlight(Json(ErrorBody::new(
            ErrorCode::IdempotencyKeyInFlight,
            "a request with this Idempotency-Key is still in flight; retry it later",
        )))
    }

    fn reused() -> Self {
        Self::Reused(Json(ErrorBody::new(
            ErrorCode::IdempotencyKeyReused,
            "this Idempotency-Key was sent with a different request",
        )))
    }
}

/// Documents `Idempotency-Key` on every unsafe operation of `T`, and the
/// refusals it brings, so an operation added later is documented as it's served.
#[derive(Default)]
pub struct Idempotent<T>(pub T);

impl<T: OpenApi> OpenApi for Idempotent<T> {
    fn meta() -> Vec<MetaApi> {
        let mut apis = T::meta();
        for operation in apis
            .iter_mut()
            .flat_map(|api| api.paths.iter_mut())
            .flat_map(|path| path.operations.iter_mut())
            .filter(|operation| is_unsafe(&operation.method))
        {
            operation.params.push(MetaOperationParam {
                name: IDEMPOTENCY_KEY.to_owned(),
                schema: String::schema_ref(),
                in_type: MetaParamIn::Header,
                description: Some(format!(
                    "Up to {MAX_KEY_LENGTH} characters, unique per request: a retry with the \
                     same key and request within {} hours gets the first successful answer \
                     replayed instead of running again.",
                    RETAINED.num_hours()
                )),
                required: false,
                deprecated: false,
                explode: true,
                style: None,
            });
            let declared: Vec<_> = operation
                .responses
                .responses
                .iter()
                .map(|response| response.status)
                .collect();
            operation.responses.responses.extend(
                IdempotencyRefusal::meta()
                    .responses
                    .into_iter()
                    .filter(|response| !declared.contains(&response.status)),
            );
            operation
                .responses
                .responses
                .sort_by_key(|response| response.status);
        }
        apis
    }

    fn register(registry: &mut Registry) {
        T::register(registry);
        IdempotencyRefusal::register(registry);
    }

    fn add_routes(
        self,
        route_table: &mut HashMap<String, HashMap<Method, poem::endpoint::BoxEndpoint<'static>>>,
    ) {
        self.0.add_routes(route_table);
    }
}
