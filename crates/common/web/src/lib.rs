mod auth;
mod caller;
mod concurrency;
mod draining;
mod error;
mod health;
mod idempotency;
mod listing;
pub mod mcp;
mod metrics;
mod provide;
mod public_url;
mod queries;
mod request_id;
#[cfg(any(test, feature = "test"))]
pub mod testing;
mod trace;

pub use app_authz::Principal;
pub use auth::{Act, Claims, JwtAuth, JwtVerifier, PrincipalResolver};
pub use caller::{Caller, Performs};
pub use concurrency::{WithETag, if_match_version};
pub use draining::Draining;
pub use error::{
    ApiError, ApiResult, ErrorBody, ErrorCode, FieldErrorBody, render_error, with_error_shape,
};
pub use health::health_routes;
pub use idempotency::{IDEMPOTENCY_KEY, Idempotency, IdempotencyEndpoint, Idempotent};
pub use listing::{ListResult, SortParam, paged, sorts};
pub use metrics::PoolWaits;
pub use provide::{Accept, Provide};
pub use public_url::PublicUrl;
pub use queries::QuerySpans;
pub use request_id::{RequestId, RequestIdMiddleware};
pub use trace::TraceRequests;
