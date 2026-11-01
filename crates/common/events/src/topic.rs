use crate::{EnvelopeOf, Event, EventEnvelope};
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Debug;
use uuid::Uuid;

/// Everything that happens to one kind of aggregate root, as Kafka would have a
/// topic per root: one stream of events, keyed by the root's id. Declared with
/// [`topic!`](crate::topic!) in the publisher's `-api`, with an exhaustive enum of
/// its events, so a new event breaks every whole-topic consumer's build.
///
/// Versioned as a whole: events and their contexts only gain fields, each
/// `#[serde(default)]`, and a new event is additive on the wire; removing one,
/// or changing the entity incompatibly, is a new topic (`users.user.v2`),
/// published beside the old one until its consumers have moved.
pub trait Topic: Send + Sync + 'static {
    /// `<module>.<root>.v<n>`.
    const NAME: &'static str;
    /// The publishing module: `/<module>`.
    const SOURCE: &'static str;
    /// Every event type, one per variant of [`Event`](Self::Event).
    const TYPES: &'static [&'static str];
    /// The root's state, every property but the audit ones.
    type Entity: Serialize + DeserializeOwned + Debug + Clone + PartialEq + Send + Sync + 'static;
    /// One variant per event, holding its context.
    type Event: Debug + Clone + PartialEq + Send + Sync + 'static;

    /// The root's id, which orders its events.
    fn key(entity: &Self::Entity) -> String;

    fn event_type(event: &Self::Event) -> &'static str;

    /// The event's context as JSON, `null` for none.
    fn encode_context(event: &Self::Event) -> serde_json::Result<Value>;

    fn decode(event_type: &str, context: Value) -> Result<Self::Event, DecodeError>;
}

/// What a queue subscribes to: a whole [`Topic`], or one [`Event`] of it.
pub trait Subscribable: Send + Sync + 'static {
    type Topic: Topic;
    /// What its handler receives.
    type Envelope: Send + 'static;
    /// The event types its queue is delivered.
    const TYPES: &'static [&'static str];

    /// Refuses a delivery of another topic, or of a type not in [`TYPES`](Self::TYPES).
    fn decode(delivery: Value) -> Result<Self::Envelope, DecodeError>;
}

impl<E: Event> Subscribable for E {
    type Topic = E::Topic;
    type Envelope = EnvelopeOf<E>;
    const TYPES: &'static [&'static str] = &[E::TYPE];

    fn decode(delivery: Value) -> Result<EnvelopeOf<E>, DecodeError> {
        let envelope: EnvelopeOf<E> = serde_json::from_value(delivery)?;
        expect(&envelope, E::Topic::NAME, Some(E::TYPE))?;
        Ok(envelope)
    }
}

/// One event of a whole topic, as a subscriber to it receives it: the variant
/// it matches, with no wildcard arm, and the root's state.
#[derive(Debug, Clone, PartialEq)]
pub struct TopicEnvelope<T: Topic> {
    /// Unique per event; a redelivery carries the same one.
    pub id: Uuid,
    /// The root's id.
    pub key: String,
    /// The root's version, which orders the key's events.
    pub version: i64,
    /// When it was recorded; it doesn't order events.
    pub time: DateTime<Utc>,
    pub entity: T::Entity,
    pub event: T::Event,
}

#[derive(Deserialize)]
struct RawPayload<E> {
    entity: E,
    #[serde(default)]
    context: Value,
}

/// Decodes in two steps, because a serde-tagged enum can't reach a `type` and
/// a `context` at different depths: the envelope with its context as raw JSON,
/// then the variant its type names.
pub fn decode_topic<T: Topic>(delivery: Value) -> Result<TopicEnvelope<T>, DecodeError> {
    let envelope: EventEnvelope<RawPayload<T::Entity>> = serde_json::from_value(delivery)?;
    expect(&envelope, T::NAME, None)?;
    Ok(TopicEnvelope {
        event: T::decode(&envelope.event_type, envelope.data.context)?,
        id: envelope.id,
        key: envelope.partition_key,
        version: envelope.entity_version,
        time: envelope.time,
        entity: envelope.data.entity,
    })
}

fn expect<D>(
    envelope: &EventEnvelope<D>,
    topic: &'static str,
    event_type: Option<&'static str>,
) -> Result<(), DecodeError> {
    let other_type = event_type.is_some_and(|t| t != envelope.event_type);
    if envelope.topic != topic || other_type {
        return Err(DecodeError::NotSubscribed {
            topic: envelope.topic.clone(),
            event_type: envelope.event_type.clone(),
        });
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// E.g. a delivery to a queue whose name was reused for another
    /// subscription while deliveries were pending.
    #[error("this queue isn't subscribed to `{event_type}` of `{topic}`")]
    NotSubscribed { topic: String, event_type: String },
    #[error("`{0}` is no event of this topic")]
    UnknownType(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Declares a topic in a publisher's `-api`: the [`Topic`], its exhaustive
/// event enum with a fieldless `…Kind` twin, and one [`Event`] marker per
/// variant, for subscribers that want only that one. A variant holds its
/// context, if it has one. Fails to compile if two events share a type.
///
/// ```
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// pub struct List { pub id: u32 }
///
/// #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// #[serde(rename_all = "camelCase")]
/// pub struct ItemRemoved { pub item_id: u32 }
///
/// app_events::topic! {
///     /// Every change to a to-do list.
///     pub topic ListTopic("lists.list.v1") {
///         source: "/lists",
///         entity: List,
///         key: |list| list.id.to_string(),
///         events: ListEvent, ListEventKind;
///         ListCreated => Created: "lists.list_created",
///         ItemRemovedFromList => ItemRemoved(ItemRemoved): "lists.item_removed",
///     }
/// }
///
/// let (event_type, context) = ListEvent::ItemRemoved(ItemRemoved { item_id: 7 }).encode().unwrap();
/// assert_eq!(event_type, "lists.item_removed");
/// assert_eq!(ListEvent::decode(event_type, context).unwrap().kind(), ListEventKind::ItemRemoved);
/// ```
#[macro_export]
macro_rules! topic {
    (
        $(#[$meta:meta])*
        $vis:vis topic $topic:ident($name:literal) {
            source: $source:literal,
            entity: $entity:ty,
            key: |$root:ident| $key:expr,
            events: $event:ident, $kind:ident;
            $( $(#[$event_meta:meta])* $marker:ident => $variant:ident $(($context:ty))?: $type:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        $vis struct $topic;

        /// Every event of the topic, holding its context.
        #[derive(Debug, Clone, PartialEq)]
        $vis enum $event {
            $( $(#[$event_meta])* $variant $(($context))? ),+
        }

        /// Which event, without its context.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $kind {
            $( $variant ),+
        }

        const _: () = assert!(
            $crate::__private::unique(&[$($type),+]),
            concat!("two events of ", $name, " share a type"),
        );

        impl $kind {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn event_type(self) -> &'static str {
                match self {
                    $( Self::$variant => $type ),+
                }
            }
        }

        impl $event {
            pub fn kind(&self) -> $kind {
                match self {
                    $( $crate::__topic_variant!(pattern Self::$variant, _context $(, $context)?) => $kind::$variant ),+
                }
            }

            pub fn decode(
                event_type: &str,
                #[allow(unused_variables)] context: $crate::__private::serde_json::Value,
            ) -> ::std::result::Result<Self, $crate::DecodeError> {
                match event_type {
                    $( $type => $crate::__topic_variant!(decode Self::$variant, context $(, $context)?), )+
                    other => Err($crate::DecodeError::UnknownType(other.to_owned())),
                }
            }

            /// Its type, and its context as JSON (`null` for none).
            pub fn encode(
                &self,
            ) -> $crate::__private::serde_json::Result<(&'static str, $crate::__private::serde_json::Value)> {
                Ok((self.kind().event_type(), <$topic as $crate::Topic>::encode_context(self)?))
            }
        }

        impl $crate::Topic for $topic {
            const NAME: &'static str = $name;
            const SOURCE: &'static str = $source;
            const TYPES: &'static [&'static str] = &[$($type),+];
            type Entity = $entity;
            type Event = $event;

            fn key($root: &$entity) -> String {
                $key
            }

            fn event_type(event: &$event) -> &'static str {
                event.kind().event_type()
            }

            fn encode_context(
                event: &$event,
            ) -> $crate::__private::serde_json::Result<$crate::__private::serde_json::Value> {
                match event {
                    $( $crate::__topic_variant!(pattern $event::$variant, context $(, $context)?) =>
                        $crate::__topic_variant!(encode context $(, $context)?), )+
                }
            }

            fn decode(
                event_type: &str,
                context: $crate::__private::serde_json::Value,
            ) -> ::std::result::Result<$event, $crate::DecodeError> {
                $event::decode(event_type, context)
            }
        }

        impl $crate::Subscribable for $topic {
            type Topic = Self;
            type Envelope = $crate::TopicEnvelope<Self>;
            const TYPES: &'static [&'static str] = <Self as $crate::Topic>::TYPES;

            fn decode(
                delivery: $crate::__private::serde_json::Value,
            ) -> ::std::result::Result<Self::Envelope, $crate::DecodeError> {
                $crate::__private::decode_topic(delivery)
            }
        }

        $(
            $(#[$event_meta])*
            $vis struct $marker;

            impl $crate::Event for $marker {
                const TYPE: &'static str = $type;
                type Topic = $topic;
                type Context = $crate::__topic_variant!(context $(, $context)?);
            }
        )+
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __topic_variant {
    (pattern $($path:ident)::+, $binding:ident) => { $($path)::+ };
    (pattern $($path:ident)::+, $binding:ident, $context:ty) => { $($path)::+($binding) };
    (decode $($path:ident)::+, $value:ident) => { Ok($($path)::+) };
    (decode $($path:ident)::+, $value:ident, $context:ty) => {
        $crate::__private::serde_json::from_value::<$context>($value)
            .map($($path)::+)
            .map_err($crate::DecodeError::from)
    };
    (encode $binding:ident) => {
        Ok::<_, $crate::__private::serde_json::Error>($crate::__private::serde_json::Value::Null)
    };
    (encode $binding:ident, $context:ty) => { $crate::__private::serde_json::to_value($binding) };
    (context) => { $crate::NoContext };
    (context, $context:ty) => { $context };
}
