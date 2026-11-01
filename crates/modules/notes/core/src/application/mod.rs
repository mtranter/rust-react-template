//! `Version`, `Versioned`, `Expected` and the listing types are re-exported from `app-persistence`
//! so they read as this layer's vocabulary while staying identical across modules.

mod error;
mod repository;
mod service;

pub use app_persistence::{Expected, ListQuery, Listed, Version, Versioned};
pub use repository::{NoteRepository, NoteSortKey};
pub use service::NoteService;
