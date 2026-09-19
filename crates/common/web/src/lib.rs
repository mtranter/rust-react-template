//! `app-web` — the poem-coupled cross-cutting layer.
//!
//! Owns the HTTP translation of `AppError`, the request-id middleware, health
//! routes, pagination types, and the JWT security scheme. Module **api** layers
//! and the **server** depend on this; the application and domain layers never do.

mod auth;
mod concurrency;
mod error;
mod health;
mod pagination;
mod provide;
mod request_id;

pub use auth::{Claims, JwtAuth, JwtVerifier};
pub use concurrency::{WithETag, if_match_version};
pub use error::{ApiResultExt, WebError};
pub use health::health_routes;
pub use pagination::{DEFAULT_LIMIT, MAX_LIMIT, Page, Pagination};
pub use provide::{Accept, Provide};
pub use request_id::{RequestId, RequestIdMiddleware};
