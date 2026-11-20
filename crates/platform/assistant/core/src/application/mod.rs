//! `Version`, `Versioned`, `Expected`, `Tx` and the listing types are re-exported from
//! `app-persistence` so they read as this layer's vocabulary while staying identical across
//! modules.

mod conversations;
mod error;
mod events;
mod model;
mod repository;
mod service;
mod settings;

pub use app_persistence::{
    EventPublisher, Expected, ListQuery, Listed, PendingEvent, Transactions, Tx, Version, Versioned,
};
#[cfg(feature = "seed")]
pub(crate) use events::pending;
pub use model::{ChatModel, ModelError, ModelMessage, ModelReply, ModelRequest};
pub use repository::{ConversationRepository, ConversationSortKey, ConversationSummary};
pub use service::{AssistantService, Status};
pub use settings::Settings;
