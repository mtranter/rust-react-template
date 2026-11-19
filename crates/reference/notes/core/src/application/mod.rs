//! `Version`, `Versioned`, `Expected`, `Tx` and the listing types are re-exported from `app-persistence`
//! so they read as this layer's vocabulary while staying identical across modules.

mod error;
mod events;
mod jobs;
mod reactions;
mod repository;
mod service;

pub use app_persistence::{
    DeferredJobs, EventPublisher, Expected, ListQuery, Listed, PendingEvent, Transactions, Tx,
    Version, Versioned,
};
#[cfg(feature = "seed")]
pub(crate) use events::pending;
pub use jobs::ArchiveNote;
pub use repository::{NoteRepository, NoteSortKey};
pub use service::NoteService;
