//! The single mapping from the pure [`AppError`] taxonomy to HTTP responses.
//!
//! Services return `AppError`; nothing below the api layer knows about poem. Here —
//! and only here — those kinds become status codes and a stable JSON error body.

use app_error::{AppError, AppResult};
use poem::{Response, error::ResponseError, http::StatusCode};

/// poem-facing wrapper around [`AppError`].
///
/// The orphan rule forbids implementing poem's `ResponseError` directly on the
/// foreign `AppError`, so this thin newtype carries the impl. `#[error(transparent)]`
/// delegates `Display`/`source` to the inner error for logging.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct WebError(pub AppError);

impl From<AppError> for WebError {
    fn from(err: AppError) -> Self {
        WebError(err)
    }
}

impl WebError {
    /// Stable machine-readable error code for the response body.
    fn code(&self) -> &'static str {
        match &self.0 {
            AppError::NotFound(_) => "not_found",
            AppError::Validation(_) => "validation_error",
            AppError::Unauthorized => "unauthorized",
            AppError::Forbidden => "forbidden",
            AppError::Conflict(_) => "conflict",
            AppError::PreconditionRequired(_) => "precondition_required",
            AppError::PreconditionFailed(_) => "precondition_failed",
            AppError::Internal(_) => "internal_error",
        }
    }
}

impl ResponseError for WebError {
    fn status(&self) -> StatusCode {
        match &self.0 {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden => StatusCode::FORBIDDEN,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::PreconditionRequired(_) => StatusCode::PRECONDITION_REQUIRED,
            AppError::PreconditionFailed(_) => StatusCode::PRECONDITION_FAILED,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn as_response(&self) -> Response {
        let status = self.status();
        // Never leak internal error detail to clients; log it instead.
        let message = match &self.0 {
            AppError::Internal(source) => {
                tracing::error!(error = %source, "internal server error");
                "internal error".to_string()
            }
            other => other.to_string(),
        };
        let body = serde_json::json!({
            "error": { "code": self.code(), "message": message }
        });
        Response::builder()
            .status(status)
            .content_type("application/json")
            .body(body.to_string())
    }
}

/// Ergonomic bridge from a service `AppResult` to a poem handler result.
///
/// Lets an operation write `repo.find(id).await.api()?` and get correct HTTP
/// status mapping for free, while still using `?` on poem extractor errors.
pub trait ApiResultExt<T> {
    fn api(self) -> poem::Result<T>;
}

impl<T> ApiResultExt<T> for AppResult<T> {
    fn api(self) -> poem::Result<T> {
        self.map_err(|e| WebError(e).into())
    }
}
