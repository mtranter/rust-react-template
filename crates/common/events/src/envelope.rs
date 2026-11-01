use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The one shape every event is stored and delivered in: a CloudEvents 1.0
/// envelope (its JSON format) around the event's `data`, its [`EventPayload`](crate::EventPayload).
/// Subscribers handle `EnvelopeOf<E>`, so they get the id to deduplicate on and
/// the time it happened.
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
    /// The id of the aggregate the event is about.
    pub subject: String,
    /// When the unit of work that published it committed.
    pub time: DateTime<Utc>,
    #[serde(rename = "datacontenttype")]
    pub data_content_type: String,
    pub data: T,
}

const SPEC_VERSION: &str = "1.0";
const DATA_CONTENT_TYPE: &str = "application/json";

impl<T> EventEnvelope<T> {
    pub fn new(
        id: Uuid,
        time: DateTime<Utc>,
        source: &str,
        event_type: &str,
        subject: String,
        data: T,
    ) -> Self {
        Self {
            spec_version: SPEC_VERSION.into(),
            id,
            source: source.into(),
            event_type: event_type.into(),
            subject,
            time,
            data_content_type: DATA_CONTENT_TYPE.into(),
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EnvelopeOf, Event, EventPayload, NoContext, PayloadOf};
    use serde_json::json;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct List {
        id: Uuid,
        item_ids: Vec<Uuid>,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ItemRemoved {
        item_id: Uuid,
    }

    struct ListRenamed;

    impl Event for ListRenamed {
        const TYPE: &'static str = "lists.list_renamed.v1";
        const SOURCE: &'static str = "/lists";
        type Entity = List;
        type Context = NoContext;

        fn subject(list: &List) -> String {
            list.id.to_string()
        }
    }

    struct ItemRemovedFromList;

    impl Event for ItemRemovedFromList {
        const TYPE: &'static str = "lists.item_removed.v1";
        const SOURCE: &'static str = "/lists";
        type Entity = List;
        type Context = ItemRemoved;

        fn subject(list: &List) -> String {
            list.id.to_string()
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

    /// As the commit stamps it.
    fn wrap<E: Event>(payload: PayloadOf<E>) -> EnvelopeOf<E> {
        let subject = E::subject(&payload.entity);
        let epoch = DateTime::from_timestamp(0, 0).unwrap();
        EventEnvelope::new(Uuid::nil(), epoch, E::SOURCE, E::TYPE, subject, payload)
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
                "type": "lists.list_renamed.v1",
                "subject": LIST.to_string(),
                "time": "1970-01-01T00:00:00Z",
                "datacontenttype": "application/json",
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
}
