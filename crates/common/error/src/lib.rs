//! Pure on purpose: the HTTP mapping lives in `app-web`, so application and
//! domain layers can return these without knowing poem.

use std::error::Error as StdError;

/// Each variant maps to exactly one HTTP status in `app-web`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0} not found")]
    NotFound(String),

    /// `fields` names the inputs at fault, when the failure can be pinned to them.
    #[error("invalid input: {message}")]
    Validation {
        message: String,
        fields: Vec<FieldError>,
    },

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("conflict: {0}")]
    Conflict(String),

    /// A write arrived without the `If-Match` version.
    #[error("precondition required: {0}")]
    PreconditionRequired(String),

    /// A stale `If-Match` version.
    #[error("precondition failed: {0}")]
    PreconditionFailed(String),

    /// The source is logged but never sent to clients.
    #[error("internal error")]
    Internal(#[source] Box<dyn StdError + Send + Sync + 'static>),
}

impl AppError {
    pub fn internal<E>(err: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        AppError::Internal(Box::new(err))
    }

    pub fn not_found(entity: impl Into<String>) -> Self {
        AppError::NotFound(entity.into())
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        AppError::Validation {
            message: msg.into(),
            fields: Vec::new(),
        }
    }

    /// `path` names the input as the client sent it (e.g. `title`, `author.email`),
    /// so a form can show `msg` beside it.
    pub fn invalid_field(path: impl Into<String>, msg: impl Into<String>) -> Self {
        let message = msg.into();
        AppError::Validation {
            fields: vec![FieldError {
                path: path.into(),
                message: message.clone(),
            }],
            message,
        }
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        AppError::Conflict(msg.into())
    }

    pub fn precondition_required(msg: impl Into<String>) -> Self {
        AppError::PreconditionRequired(msg.into())
    }

    pub fn precondition_failed(msg: impl Into<String>) -> Self {
        AppError::PreconditionFailed(msg.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub path: String,
    pub message: String,
}

pub type AppResult<T> = Result<T, AppError>;
