//! What modules announce to each other (spec §4): the [`Topic`] each aggregate
//! root's events belong to, each [`Event`] of it, the [`EventPayload`] every
//! event carries and the [`EventEnvelope`] it travels in. `app-persistence`
//! records them in a unit of work; `app-db` delivers them.

mod envelope;
mod payload;
mod topic;

pub use envelope::{EventAttributes, EventEnvelope};
pub use payload::{EventPayload, NoContext};
pub use topic::{DecodeError, Subscribable, Topic, TopicEnvelope};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// One kind of thing that happens to a topic's root, announced for modules to
/// react to: a fact in the past tense, never a command in disguise. Declared,
/// with its topic, by [`topic!`]: a marker type naming its type string and its
/// context, which says what happened where the root's state alone can't
/// (`ToDoItemRemoved` carries the list, and the removed item's id as its context).
pub trait Event: Send + Sync + 'static {
    /// Stable across releases, unique within its topic: `<module>.<event>`.
    const TYPE: &'static str;
    type Topic: Topic;
    /// [`NoContext`] for an event the entity says everything about.
    type Context: Serialize + DeserializeOwned + Send + Sync + 'static;
}

/// The root's state every event of `E`'s topic carries.
pub type EntityOf<E> = <<E as Event>::Topic as Topic>::Entity;

/// What `E` carries.
pub type PayloadOf<E> = EventPayload<EntityOf<E>, <E as Event>::Context>;

/// `E` as it's stored and delivered.
pub type EnvelopeOf<E> = EventEnvelope<PayloadOf<E>>;

#[doc(hidden)]
pub mod __private {
    pub use crate::topic::decode_topic;
    pub use serde_json;

    /// Whether no two of `types` are equal; `topic!` asserts it at compile time.
    pub const fn unique(types: &[&str]) -> bool {
        let mut i = 0;
        while i < types.len() {
            let mut j = i + 1;
            while j < types.len() {
                if same(types[i], types[j]) {
                    return false;
                }
                j += 1;
            }
            i += 1;
        }
        true
    }

    const fn same(a: &str, b: &str) -> bool {
        let (a, b) = (a.as_bytes(), b.as_bytes());
        if a.len() != b.len() {
            return false;
        }
        let mut i = 0;
        while i < a.len() {
            if a[i] != b[i] {
                return false;
            }
            i += 1;
        }
        true
    }
}
