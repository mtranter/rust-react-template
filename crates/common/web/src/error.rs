//! One error shape for every failure: handlers return [`ApiError`], which each
//! operation declares in the spec, and [`render_error`] turns everything else
//! (unparseable requests, unknown routes, failed auth) into the same body.

use crate::request_id::current_request_id;
use app_error::AppError;
use poem::{IntoResponse, Response, http::StatusCode};
use poem_openapi::{ApiResponse, Enum, Object, payload::Json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
#[oai(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Well-formed, but breaks a rule; `fields` says where when it can.
    ValidationError,
    /// Unparseable: bad JSON, a missing field, an unknown enum value.
    MalformedRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    Conflict,
    UnsupportedMediaType,
    PreconditionRequired,
    PreconditionFailed,
    InternalError,
}

#[derive(Debug, Clone, Object)]
#[oai(rename_all = "camelCase")]
pub struct ErrorBody {
    /// Branch on this, not on `message`.
    pub code: ErrorCode,
    pub message: String,
    #[oai(skip_serializing_if_is_none)]
    pub fields: Option<Vec<FieldErrorBody>>,
    /// The same id as the `x-request-id` header.
    #[oai(skip_serializing_if_is_none)]
    pub request_id: Option<String>,
}

/// One input at fault. `path` names it as the client sent it (e.g. `title`).
#[derive(Debug, Clone, Object)]
#[oai(rename = "FieldError")]
pub struct FieldErrorBody {
    pub path: String,
    pub message: String,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            fields: None,
            request_id: current_request_id(),
        }
    }
}

/// Every operation returns `Result<_, ApiError>`, so every operation declares
/// the same error responses; `?` converts an [`AppError`].
#[derive(ApiResponse)]
pub enum ApiError {
    /// Malformed or invalid.
    #[oai(status = 400)]
    BadRequest(Json<ErrorBody>),
    #[oai(status = 401)]
    Unauthorized(Json<ErrorBody>),
    #[oai(status = 403)]
    Forbidden(Json<ErrorBody>),
    #[oai(status = 404)]
    NotFound(Json<ErrorBody>),
    #[oai(status = 409)]
    Conflict(Json<ErrorBody>),
    /// The `If-Match` version is stale.
    #[oai(status = 412)]
    PreconditionFailed(Json<ErrorBody>),
    /// A write arrived without `If-Match`.
    #[oai(status = 428)]
    PreconditionRequired(Json<ErrorBody>),
    #[oai(status = 500)]
    Internal(Json<ErrorBody>),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        let message = err.to_string();
        match err {
            AppError::Validation { message, fields } => {
                let mut body = ErrorBody::new(ErrorCode::ValidationError, message);
                if !fields.is_empty() {
                    body.fields = Some(
                        fields
                            .into_iter()
                            .map(|f| FieldErrorBody {
                                path: f.path,
                                message: f.message,
                            })
                            .collect(),
                    );
                }
                ApiError::BadRequest(Json(body))
            }
            AppError::NotFound(_) => {
                ApiError::NotFound(Json(ErrorBody::new(ErrorCode::NotFound, message)))
            }
            AppError::Unauthorized => {
                ApiError::Unauthorized(Json(ErrorBody::new(ErrorCode::Unauthorized, message)))
            }
            AppError::Forbidden => {
                ApiError::Forbidden(Json(ErrorBody::new(ErrorCode::Forbidden, message)))
            }
            AppError::Conflict(_) => {
                ApiError::Conflict(Json(ErrorBody::new(ErrorCode::Conflict, message)))
            }
            AppError::PreconditionRequired(_) => ApiError::PreconditionRequired(Json(
                ErrorBody::new(ErrorCode::PreconditionRequired, message),
            )),
            AppError::PreconditionFailed(_) => ApiError::PreconditionFailed(Json(ErrorBody::new(
                ErrorCode::PreconditionFailed,
                message,
            ))),
            AppError::Internal(source) => {
                tracing::error!(error = %source, "internal server error");
                ApiError::Internal(Json(ErrorBody::new(ErrorCode::InternalError, message)))
            }
        }
    }
}

/// For `catch_all_error`: passes an [`ApiError`] through and renders any other
/// error (raised by poem or poem-openapi before a handler runs) as an
/// [`ErrorBody`] with the same status.
pub async fn render_error(err: poem::Error) -> Response {
    if err.is_from_response() {
        return err.into_response();
    }
    let status = err.status();
    let code = match status {
        StatusCode::UNAUTHORIZED => ErrorCode::Unauthorized,
        StatusCode::FORBIDDEN => ErrorCode::Forbidden,
        StatusCode::NOT_FOUND => ErrorCode::NotFound,
        StatusCode::METHOD_NOT_ALLOWED => ErrorCode::MethodNotAllowed,
        StatusCode::CONFLICT => ErrorCode::Conflict,
        StatusCode::UNSUPPORTED_MEDIA_TYPE => ErrorCode::UnsupportedMediaType,
        StatusCode::PRECONDITION_REQUIRED => ErrorCode::PreconditionRequired,
        StatusCode::PRECONDITION_FAILED => ErrorCode::PreconditionFailed,
        s if s.is_client_error() => ErrorCode::MalformedRequest,
        _ => ErrorCode::InternalError,
    };
    let message = if code == ErrorCode::InternalError {
        tracing::error!(error = %err, %status, "unhandled server error");
        "internal error".to_string()
    } else {
        err.to_string()
    };
    let mut resp = Json(ErrorBody::new(code, message)).into_response();
    resp.set_status(status);
    resp
}
