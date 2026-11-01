//! Publishing events (spec §4). Services hold an `Arc<dyn EventPublisher>` beside
//! their repository and publish into the same [`Tx`] they save in, so an event is
//! delivered if and only if the change it announces commits.

use crate::{Tx, Version};
use app_error::{AppError, AppResult};
use app_events::{EventAttributes, EventEnvelope, Topic};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use uuid::Uuid;

#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// Delivered to every subscriber, at least once, if and only if `tx` commits:
    /// `publisher.publish(&mut tx, PendingEvent::on::<UserTopic>(version, &user, UserEvent::Registered))`.
    async fn publish(&self, tx: &mut Tx, event: PendingEvent) -> AppResult<()>;
}

/// An event to publish, its topic erased so every module's events share one
/// type; the publisher stamps it with an id and time to make its
/// [`EventEnvelope`].
#[derive(Debug)]
pub struct PendingEvent {
    attributes: EventAttributes,
    /// Serialized here, but failing only at [`EventPublisher::publish`], so
    /// building a `PendingEvent` can't.
    data: serde_json::Result<Value>,
}

impl PendingEvent {
    /// `event` made the root of `T` `state`, at `version`: the one its save
    /// returned, or for a deletion the one deleted, [`next`](Version::next).
    pub fn on<T: Topic>(version: Version, state: &T::Entity, event: T::Event) -> Self {
        let attributes = EventAttributes {
            source: T::SOURCE,
            topic: T::NAME,
            event_type: T::event_type(&event),
            key: T::key(state),
            version: version.as_i64(),
        };
        let data = T::encode_context(&event).and_then(|context| {
            let mut data = Map::new();
            data.insert("entity".into(), serde_json::to_value(state)?);
            if !context.is_null() {
                data.insert("context".into(), context);
            }
            Ok(Value::Object(data))
        });
        Self { attributes, data }
    }

    pub fn attributes(&self) -> &EventAttributes {
        &self.attributes
    }

    pub fn envelope(self, id: Uuid, time: DateTime<Utc>) -> AppResult<EventEnvelope<Value>> {
        let data = self.data.map_err(AppError::internal)?;
        Ok(EventEnvelope::new(id, time, self.attributes, data))
    }
}
