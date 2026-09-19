//! Application layer: services that coordinate each use case. A service asks the
//! `Repository` for an aggregate, runs a domain action on it, and saves the
//! result. Rules that span more than one aggregate (e.g. email uniqueness) live
//! here, not in the domain. Depends on the domain and pure crates only.
//!
//! - `service` — `UserService`, one method per use case.
//! - `repository` — the persistence interface the db layer implements.
//! - `error` — the mapping from domain errors onto `AppError`.
//!
//! Optimistic-concurrency primitives (`Version`, `Versioned`, `Expected`) are
//! re-exported from the shared pure `app-persistence` crate so they read as this
//! layer's own vocabulary while staying identical across modules.

mod error;
mod repository;
mod service;

pub use app_persistence::{Expected, Version, Versioned};
pub use repository::UserRepository;
pub use service::UserService;
