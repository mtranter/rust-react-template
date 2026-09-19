//! HTTP surface for optimistic concurrency: the `ETag`/`If-Match` translation of
//! the pure [`Version`] vocabulary from `app-persistence`.
//!
//! A module returns [`WithETag<T>`] from single-item reads and writes so the
//! response always carries the version — both in the body (`T` mirrors it) and
//! in a strong `ETag` header. A write reads the required version back from
//! `If-Match` via [`if_match_version`]; the version never travels in the request
//! body, so there is a single authoritative source.

use app_error::AppError;
use app_persistence::Version;
use poem_openapi::{
    ApiResponse,
    payload::Json,
    types::{ParseFromJSON, ToJSON, Type},
};

use crate::ApiResultExt;

/// A single resource plus its optimistic-concurrency [`Version`] as a strong
/// `ETag` header. The body type `T` is expected to also mirror the version as a
/// field, so a type-safe generated client can read it without touching headers.
#[derive(ApiResponse)]
pub enum WithETag<T: ParseFromJSON + ToJSON + Type + Send> {
    #[oai(status = 200)]
    Ok(Json<T>, #[oai(header = "ETag")] String),
}

impl<T: ParseFromJSON + ToJSON + Type + Send> WithETag<T> {
    /// Wrap a response body with its version. The `ETag` is the version as a
    /// quoted opaque token (RFC 9110 §8.8.3).
    pub fn new(body: T, version: Version) -> Self {
        WithETag::Ok(Json(body), format!("\"{}\"", version.as_i64()))
    }
}

/// The version a conditional write must match, parsed from an `If-Match` header.
///
/// Absent ⇒ `428 Precondition Required` (we never fall back to a blind
/// overwrite); malformed ⇒ `400`. Accepts an optional weak prefix and the
/// surrounding quotes of a normal ETag, so a client can echo back exactly the
/// token it received.
pub fn if_match_version(raw: Option<String>) -> poem::Result<Version> {
    let raw = raw
        .ok_or_else(|| AppError::precondition_required("If-Match header is required to edit"))
        .api()?;
    let token = raw.trim().trim_start_matches("W/").trim_matches('"');
    token
        .parse::<i64>()
        .map(Version::from_i64)
        .map_err(|_| AppError::validation("If-Match must be a resource version"))
        .api()
}
