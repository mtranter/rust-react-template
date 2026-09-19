//! Canonical, framework-agnostic error kinds.
//!
//! This is a **pure** crate: it knows nothing about HTTP or SQL. The application
//! layer of every module returns [`AppError`] (mapping its domain errors onto
//! it); `app-web` owns the single mapping from these kinds to HTTP responses.
//! Keeping the taxonomy here (and the HTTP translation in `app-web`) is what
//! lets the application and domain layers stay poem-free.

use std::error::Error as StdError;

/// The canonical error taxonomy shared across modules.
///
/// Each variant maps to exactly one HTTP status in `app-web`. Add variants here
/// (not ad-hoc strings) so the mapping stays exhaustive and total.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The requested entity does not exist.
    #[error("{0} not found")]
    NotFound(String),

    /// The request was well-formed but semantically invalid.
    #[error("invalid input: {0}")]
    Validation(String),

    /// Authentication is missing or invalid.
    #[error("unauthorized")]
    Unauthorized,

    /// The caller is authenticated but not permitted.
    #[error("forbidden")]
    Forbidden,

    /// The request conflicts with current state (e.g. unique violation).
    #[error("conflict: {0}")]
    Conflict(String),

    /// A required precondition was not supplied — e.g. a write arrived without
    /// the `If-Match` version that optimistic concurrency demands.
    #[error("precondition required: {0}")]
    PreconditionRequired(String),

    /// A supplied precondition did not hold — the resource was modified
    /// concurrently since the caller last read it (a stale `If-Match` version).
    #[error("precondition failed: {0}")]
    PreconditionFailed(String),

    /// An unexpected failure. The source is preserved for logging but never
    /// leaked to clients (`app-web` renders a generic 500).
    #[error("internal error")]
    Internal(#[source] Box<dyn StdError + Send + Sync + 'static>),
}

impl AppError {
    /// Wrap any error as an [`AppError::Internal`]. Use at the boundary where a
    /// lower-level error (e.g. `sqlx::Error`) has no more specific mapping.
    pub fn internal<E>(err: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        AppError::Internal(Box::new(err))
    }

    /// Convenience for a not-found on a named entity.
    pub fn not_found(entity: impl Into<String>) -> Self {
        AppError::NotFound(entity.into())
    }

    /// Convenience for a validation failure.
    pub fn validation(msg: impl Into<String>) -> Self {
        AppError::Validation(msg.into())
    }

    /// Convenience for a conflict.
    pub fn conflict(msg: impl Into<String>) -> Self {
        AppError::Conflict(msg.into())
    }

    /// Convenience for a missing precondition (maps to 428).
    pub fn precondition_required(msg: impl Into<String>) -> Self {
        AppError::PreconditionRequired(msg.into())
    }

    /// Convenience for a failed precondition (maps to 412).
    pub fn precondition_failed(msg: impl Into<String>) -> Self {
        AppError::PreconditionFailed(msg.into())
    }
}

/// Ergonomic result alias used throughout the application and db layers.
pub type AppResult<T> = Result<T, AppError>;
