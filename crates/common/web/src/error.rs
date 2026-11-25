//! One error shape for every failure: handlers return [`ApiError`], which each
//! operation declares in the spec, and [`render_error`] turns everything else
//! (unparseable requests, unknown routes, failed auth) into the same body.

use crate::request_id::current_request_id;
use crate::trace::current_trace_id;
use app_error::AppError;
use poem::{IntoResponse, PathPattern, Response, http::StatusCode};
use poem_openapi::{ApiResponse, Enum, Object, OperationId, payload::Json};

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
    /// The `Idempotency-Key` was sent before with a different request.
    IdempotencyKeyReused,
    /// The request first sent with this `Idempotency-Key` is still running.
    IdempotencyKeyInFlight,
    /// Not ready to answer yet, such as a read model still being built: retry later.
    Unavailable,
    /// The deployment lacks what the operation needs: retrying won't help.
    NotConfigured,
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
    /// The W3C trace id of the request; absent when tracing is off.
    #[oai(skip_serializing_if_is_none)]
    pub trace_id: Option<String>,
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
            trace_id: current_trace_id(),
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
    /// Not ready to answer yet: retry later.
    #[oai(status = 503)]
    Unavailable(Json<ErrorBody>),
    /// The deployment lacks what the operation needs.
    #[oai(status = 501)]
    NotConfigured(Json<ErrorBody>),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    /// What it answers, without its status: for a refusal sent where a status can't be, such as
    /// an event of a stream already under way.
    pub fn into_body(self) -> ErrorBody {
        match self {
            ApiError::BadRequest(Json(body))
            | ApiError::Unauthorized(Json(body))
            | ApiError::Forbidden(Json(body))
            | ApiError::NotFound(Json(body))
            | ApiError::Conflict(Json(body))
            | ApiError::PreconditionFailed(Json(body))
            | ApiError::PreconditionRequired(Json(body))
            | ApiError::Internal(Json(body))
            | ApiError::Unavailable(Json(body))
            | ApiError::NotConfigured(Json(body)) => body,
        }
    }
}

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
            AppError::Unavailable(_) => {
                ApiError::Unavailable(Json(ErrorBody::new(ErrorCode::Unavailable, message)))
            }
            AppError::NotConfigured(_) => {
                ApiError::NotConfigured(Json(ErrorBody::new(ErrorCode::NotConfigured, message)))
            }
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
        StatusCode::SERVICE_UNAVAILABLE => ErrorCode::Unavailable,
        StatusCode::NOT_IMPLEMENTED => ErrorCode::NotConfigured,
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
    // What the request's span is named and labelled by (`TraceRequests`).
    if let Some(route) = err.data::<PathPattern>() {
        resp.set_data(route.clone());
    }
    if let Some(operation) = err.data::<OperationId>() {
        resp.set_data(*operation);
    }
    resp
}

/// `spec`, an OpenAPI document as JSON, with [`ErrorBody`]'s schemas. poem-openapi
/// leaves out a schema no operation names, so an app with no operations yet
/// would generate a client without the error shape its frontend is built on.
/// A spec that has it is returned as it is.
pub fn with_error_shape(spec: String) -> String {
    let mut document: serde_json::Value =
        serde_json::from_str(&spec).expect("poem-openapi writes JSON");
    let schemas = &mut document["components"]["schemas"];
    if schemas.get("ErrorBody").is_some() {
        return spec;
    }
    let mut registry = poem_openapi::registry::Registry::new();
    <ErrorBody as poem_openapi::types::Type>::register(&mut registry);
    for (name, schema) in registry.schemas {
        schemas[name] = serde_json::to_value(schema).expect("a schema is JSON");
    }
    serde_json::to_string_pretty(&document).expect("a spec is JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use poem_openapi::{OpenApi, OpenApiService};

    struct NoOperations;

    #[OpenApi]
    impl NoOperations {}

    #[test]
    fn a_spec_without_operations_still_has_the_error_shape() {
        let spec = OpenApiService::new(NoOperations, "app", "1").spec();
        let document: serde_json::Value = serde_json::from_str(&with_error_shape(spec)).unwrap();
        let schemas = &document["components"]["schemas"];
        for name in ["ErrorBody", "ErrorCode", "FieldError"] {
            assert!(schemas.get(name).is_some(), "{name} is missing");
        }
    }
}
