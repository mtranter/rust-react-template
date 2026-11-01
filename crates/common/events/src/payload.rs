use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::any::TypeId;

/// The `data` of every event: the aggregate root's state, and what happened to
/// it where the state alone can't say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(serialize = "E: Serialize, C: Serialize + 'static"))]
pub struct EventPayload<E, C> {
    pub entity: E,
    #[serde(skip_serializing_if = "is_no_context")]
    pub context: C,
}

impl<E> EventPayload<E, NoContext> {
    pub fn new(entity: E) -> Self {
        Self {
            entity,
            context: NoContext,
        }
    }
}

impl<E, C> EventPayload<E, C> {
    pub fn with_context(entity: E, context: C) -> Self {
        Self { entity, context }
    }
}

/// The context of an event its entity says everything about: left out of the
/// JSON, and read back from a missing `context`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoContext;

fn is_no_context<C: 'static>(_: &C) -> bool {
    TypeId::of::<C>() == TypeId::of::<NoContext>()
}

impl Serialize for NoContext {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_none()
    }
}

/// Through `Option`, because serde deserializes a missing field as an absent
/// option and fails it for anything else.
impl<'de> Deserialize<'de> for NoContext {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<IgnoredAny>::deserialize(deserializer).map(|_| NoContext)
    }
}
