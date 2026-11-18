mod auth;
mod caller;
mod concurrency;
mod error;
mod health;
mod idempotency;
mod listing;
pub mod mcp;
mod provide;
mod public_url;
mod request_id;
#[cfg(any(test, feature = "test"))]
pub mod testing;

pub use app_authz::Principal;
pub use auth::{Claims, ClaimsResolver, JwtAuth, JwtVerifier, PrincipalResolver};
pub use caller::{Caller, Gate, Performs};
pub use concurrency::{WithETag, if_match_version};
pub use error::{
    ApiError, ApiResult, ErrorBody, ErrorCode, FieldErrorBody, render_error, with_error_shape,
};
pub use health::health_routes;
pub use idempotency::{IDEMPOTENCY_KEY, Idempotency, IdempotencyEndpoint, Idempotent};
pub use listing::{ListResult, SortParam, paged, sorts};
pub use provide::{Accept, Provide};
pub use public_url::PublicUrl;
pub use request_id::{RequestId, RequestIdMiddleware};
