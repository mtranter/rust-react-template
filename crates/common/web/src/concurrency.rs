//! Writes take the version only from `If-Match`, never the request body, so
//! there is a single authoritative source.

use app_error::AppError;
use app_persistence::Version;
use poem_openapi::{
    ApiResponse,
    payload::Json,
    types::{ParseFromJSON, ToJSON, Type},
};

use crate::ApiResult;

/// A single resource plus its optimistic-concurrency [`Version`] as a strong
/// `ETag` header. The body type `T` is expected to also mirror the version as a
/// field, so a type-safe generated client can read it without touching headers.
#[derive(ApiResponse)]
pub enum WithETag<T: ParseFromJSON + ToJSON + Type + Send> {
    #[oai(status = 200)]
    Ok(Json<T>, #[oai(header = "ETag")] String),
}

impl<T: ParseFromJSON + ToJSON + Type + Send> WithETag<T> {
    /// The `ETag` is quoted, per RFC 9110 §8.8.3.
    pub fn new(body: T, version: Version) -> Self {
        WithETag::Ok(Json(body), format!("\"{}\"", version.as_i64()))
    }
}

/// Absent is a 428, never a blind overwrite. Accepts the weak prefix and quotes
/// so a client can echo back exactly the `ETag` it received.
pub fn if_match_version(raw: Option<String>) -> ApiResult<Version> {
    let raw =
        raw.ok_or_else(|| AppError::precondition_required("If-Match header is required to edit"))?;
    let token = raw.trim().trim_start_matches("W/").trim_matches('"');
    token
        .parse::<i64>()
        .map(Version::from_i64)
        .map_err(|_| AppError::validation("If-Match must be a resource version").into())
}
