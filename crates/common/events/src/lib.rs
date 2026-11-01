//! What modules announce to each other (spec §4): the [`Event`] contract, the
//! [`EventPayload`] every event carries and the [`EventEnvelope`] it travels in.
//! `app-persistence` records them in a unit of work; `app-db` delivers them.

mod envelope;
mod payload;

pub use envelope::EventEnvelope;
pub use payload::{EventPayload, NoContext};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Something that happened to an aggregate, announced for modules to react to:
/// a fact in the past tense, never a command in disguise. Declared in the
/// module's `-api` as a marker type naming its parts; what it carries is an
/// [`EventPayload`]: the aggregate root's state as it now is, so a subscriber
/// never has to call back (event-carried state transfer), and, where the state
/// alone can't say what happened, a context (`ToDoItemRemoved` carries the
/// list, and the removed item's id as its context).
///
/// Stored as JSON, so a version only gains fields, each `#[serde(default)]` so
/// rows written before it still parse; a breaking change is a new `TYPE`.
pub trait Event: Send + Sync + 'static {
    /// Stable across releases: `<module>.<event>.v<n>`.
    const TYPE: &'static str;
    /// The publishing module: `/<module>`.
    const SOURCE: &'static str;
    /// The aggregate root's state, every property but the audit ones; shared by
    /// all of that root's events.
    type Entity: Serialize + DeserializeOwned + Send + Sync + 'static;
    /// [`NoContext`] for an event the entity says everything about.
    type Context: Serialize + DeserializeOwned + Send + Sync + 'static;

    /// The aggregate root's id.
    fn subject(entity: &Self::Entity) -> String;
}

/// What `E` carries.
pub type PayloadOf<E> = EventPayload<<E as Event>::Entity, <E as Event>::Context>;

/// `E` as it's stored and delivered.
pub type EnvelopeOf<E> = EventEnvelope<PayloadOf<E>>;
