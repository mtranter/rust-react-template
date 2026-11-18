//! `Version`, `Versioned`, `Expected`, `Tx` and the listing types are re-exported from
//! `app-persistence` so they read as this layer's vocabulary while staying identical across modules.

mod catalog;
mod error;
mod events;
mod repository;
mod rules;
mod service;

pub use app_persistence::{
    EventPublisher, ListQuery, Listed, PendingEvent, Transactions, Tx, Version, Versioned,
};
pub use catalog::{CatalogAction, CatalogResource};
pub(crate) use events::scope_name;
pub use repository::{GrantFilter, GrantRepository, GrantSortKey};
pub use rules::{POLL_EVERY, Rules};
pub use service::{AccessService, Listing, Modules, Operation};
