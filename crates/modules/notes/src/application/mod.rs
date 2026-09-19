//! Application layer for notes: services that coordinate each use case (load an
//! aggregate, run a domain action, save it). Depends on the domain and pure
//! crates only.
//!
//! - `service` — `NoteService`, one method per use case.
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
pub use repository::NoteRepository;
pub use service::NoteService;
