use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The one shape every event is stored and delivered in: a CloudEvents 1.0
/// envelope (its JSON format) around the event's `data`, its [`EventPayload`](crate::EventPayload),
/// with extension attributes for its topic and key (CloudEvents' partitioning
/// extension). Subscribers handle `EnvelopeOf<E>`, so they get the id to
/// deduplicate on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope<T> {
    #[serde(rename = "specversion")]
    pub spec_version: String,
    /// Unique per event; a redelivery carries the same one.
    pub id: Uuid,
    /// The publishing module, `/<module>`.
    pub source: String,
    #[serde(rename = "type")]
    pub event_type: String,
    /// The id of the aggregate root the event is about: its key.
    pub subject: String,
    /// When the publishing transaction recorded it, by that instance's clock:
    /// before it committed, and no way to order events; `entity_version` is.
    pub time: DateTime<Utc>,
    #[serde(rename = "datacontenttype")]
    pub data_content_type: String,
    /// The [`Topic`](crate::Topic)'s name.
    pub topic: String,
    /// The root's id, as `subject`: what orders the topic's events.
    #[serde(rename = "partitionkey")]
    pub partition_key: String,
    /// The root's version this event announces, which orders the events of
    /// one key. An extension attribute rather than CloudEvents' `sequence`,
    /// which is a string sorted lexicographically, with no stated scope; this
    /// is an integer scoped to one `topic` and `partitionkey`.
    #[serde(rename = "entityversion")]
    pub entity_version: i64,
    pub data: T,
}

/// What an envelope says an event is, beside its id and time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventAttributes {
    pub source: &'static str,
    pub topic: &'static str,
    pub event_type: &'static str,
    pub key: String,
    pub version: i64,
}

const SPEC_VERSION: &str = "1.0";
const DATA_CONTENT_TYPE: &str = "application/json";

impl<T> EventEnvelope<T> {
    pub fn new(id: Uuid, time: DateTime<Utc>, attributes: EventAttributes, data: T) -> Self {
        Self {
            spec_version: SPEC_VERSION.into(),
            id,
            source: attributes.source.into(),
            event_type: attributes.event_type.into(),
            subject: attributes.key.clone(),
            time,
            data_content_type: DATA_CONTENT_TYPE.into(),
            topic: attributes.topic.into(),
            partition_key: attributes.key,
            entity_version: attributes.version,
            data,
        }
    }
}

#[cfg(test)]
// The topics `topic!` declares here are private, so what tests don't call is dead.
#[allow(dead_code)]
mod tests {
    use super::*;
    use crate::{
        DecodeError, EnvelopeOf, Event, EventPayload, PayloadOf, Subscribable, Topic, TopicEnvelope,
    };
    use serde_json::{Value, json};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct List {
        id: Uuid,
        item_ids: Vec<Uuid>,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ItemRemoved {
        item_id: Uuid,
    }

    crate::topic! {
        topic ListTopic("lists.list.v1") {
            source: "/lists",
            entity: List,
            key: |list| list.id.to_string(),
            events: ListEvent, ListEventKind;
            ListRenamed => Renamed: "lists.list_renamed",
            ItemRemovedFromList => ItemRemoved(ItemRemoved): "lists.item_removed",
        }
    }

    crate::topic! {
        topic OtherTopic("others.list.v1") {
            source: "/others",
            entity: List,
            key: |list| list.id.to_string(),
            events: OtherEvent, OtherEventKind;
            OtherListRenamed => Renamed: "lists.list_renamed",
        }
    }

    const LIST: Uuid = Uuid::from_u128(7);
    const ITEM: Uuid = Uuid::from_u128(8);

    fn list() -> List {
        List {
            id: LIST,
            item_ids: vec![],
        }
    }

    /// As the publisher stamps it.
    fn wrap<E: Event>(payload: PayloadOf<E>) -> EnvelopeOf<E> {
        let attributes = EventAttributes {
            source: E::Topic::SOURCE,
            topic: E::Topic::NAME,
            event_type: E::TYPE,
            key: <E::Topic as Topic>::key(&payload.entity),
            version: 3,
        };
        let epoch = DateTime::from_timestamp(0, 0).unwrap();
        EventEnvelope::new(Uuid::nil(), epoch, attributes, payload)
    }

    fn delivery<E: Event>(payload: PayloadOf<E>) -> Value {
        serde_json::to_value(wrap::<E>(payload)).unwrap()
    }

    #[test]
    fn serializes_as_a_cloudevent_in_json() {
        let envelope = wrap::<ListRenamed>(EventPayload::new(list()));

        let json = serde_json::to_value(&envelope).unwrap();

        assert_eq!(
            json,
            json!({
                "specversion": "1.0",
                "id": Uuid::nil(),
                "source": "/lists",
                "type": "lists.list_renamed",
                "subject": LIST.to_string(),
                "time": "1970-01-01T00:00:00Z",
                "datacontenttype": "application/json",
                "topic": "lists.list.v1",
                "partitionkey": LIST.to_string(),
                "entityversion": 3,
                "data": { "entity": { "id": LIST, "itemIds": [] } },
            })
        );
        assert_eq!(
            serde_json::from_value::<EnvelopeOf<ListRenamed>>(json).unwrap(),
            envelope
        );
    }

    #[test]
    fn carries_a_context_beside_the_entity() {
        let payload = EventPayload::with_context(list(), ItemRemoved { item_id: ITEM });
        let envelope = wrap::<ItemRemovedFromList>(payload);

        let json = serde_json::to_value(&envelope).unwrap();

        assert_eq!(
            json["data"],
            json!({
                "entity": { "id": LIST, "itemIds": [] },
                "context": { "itemId": ITEM },
            })
        );
        assert_eq!(
            serde_json::from_value::<EnvelopeOf<ItemRemovedFromList>>(json).unwrap(),
            envelope
        );
    }

    #[test]
    fn every_event_of_a_topic_round_trips_through_its_type_and_context() {
        let events = [
            ListEvent::Renamed,
            ListEvent::ItemRemoved(ItemRemoved { item_id: ITEM }),
        ];

        for event in &events {
            let (event_type, context) = event.encode().unwrap();
            assert_eq!(event_type, event.kind().event_type());
            assert_eq!(&ListEvent::decode(event_type, context).unwrap(), event);
        }
        assert_eq!(ListEventKind::ALL.len(), events.len());
        assert_eq!(
            <ListTopic as Topic>::TYPES,
            ["lists.list_renamed", "lists.item_removed"]
        );
    }

    #[test]
    fn a_type_the_topic_doesnt_have_doesnt_decode() {
        assert!(matches!(
            ListEvent::decode("lists.list_archived", Value::Null),
            Err(DecodeError::UnknownType(_))
        ));
    }

    #[test]
    fn a_whole_topic_subscriber_gets_each_event_as_its_variant() {
        let removed = EventPayload::with_context(list(), ItemRemoved { item_id: ITEM });

        let decoded = <ListTopic as Subscribable>::decode(delivery::<ItemRemovedFromList>(removed));

        assert_eq!(
            decoded.unwrap(),
            TopicEnvelope::<ListTopic> {
                id: Uuid::nil(),
                key: LIST.to_string(),
                version: 3,
                time: DateTime::from_timestamp(0, 0).unwrap(),
                entity: list(),
                event: ListEvent::ItemRemoved(ItemRemoved { item_id: ITEM }),
            }
        );
    }

    #[test]
    fn a_subscriber_to_one_event_refuses_another_of_its_topic() {
        let removed = EventPayload::with_context(list(), ItemRemoved { item_id: ITEM });

        let decoded =
            <ListRenamed as Subscribable>::decode(delivery::<ItemRemovedFromList>(removed));

        assert!(matches!(decoded, Err(DecodeError::NotSubscribed { .. })));
    }

    #[test]
    fn a_subscriber_refuses_another_topics_event_of_the_same_type() {
        let renamed = delivery::<OtherListRenamed>(EventPayload::new(list()));

        assert!(matches!(
            <ListRenamed as Subscribable>::decode(renamed.clone()),
            Err(DecodeError::NotSubscribed { .. })
        ));
        assert!(matches!(
            <ListTopic as Subscribable>::decode(renamed),
            Err(DecodeError::NotSubscribed { .. })
        ));
    }

    #[test]
    fn types_are_unique_unless_two_are_equal() {
        assert!(crate::__private::unique(&["a.x", "a.y", "a.xy"]));
        assert!(!crate::__private::unique(&["a.x", "a.y", "a.x"]));
    }
}
