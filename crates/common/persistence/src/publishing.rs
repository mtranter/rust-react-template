//! Publishing events (spec §4). Services hold an `Arc<dyn EventPublisher>` beside
//! their repository and publish into the same [`Tx`] they save in, so an event is
//! delivered if and only if the change it announces commits.

use crate::Tx;
use app_error::{AppError, AppResult};
use app_events::{Event, PayloadOf};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use std::fmt;

#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// Delivered to every subscriber, at least once, if and only if `tx` commits:
    /// `publisher.publish(&mut tx, PendingEvent::of::<UserRegistered>(payload))`.
    async fn publish(&self, tx: &mut Tx, event: PendingEvent) -> AppResult<()>;
}

/// An event to publish, its type erased so a module's events share one type;
/// the publisher stamps it with an id and time to make its
/// [`EventEnvelope`](app_events::EventEnvelope).
pub struct PendingEvent {
    event_type: &'static str,
    source: &'static str,
    subject: String,
    payload: Box<dyn ErasedPayload>,
}

impl PendingEvent {
    pub fn of<E: Event>(payload: PayloadOf<E>) -> Self {
        Self {
            event_type: E::TYPE,
            source: E::SOURCE,
            subject: E::subject(&payload.entity),
            payload: Box::new(payload),
        }
    }

    pub fn event_type(&self) -> &'static str {
        self.event_type
    }

    pub fn source(&self) -> &'static str {
        self.source
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn data(&self) -> AppResult<Value> {
        self.payload.to_json().map_err(AppError::internal)
    }
}

impl fmt::Debug for PendingEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingEvent")
            .field("event_type", &self.event_type)
            .field("subject", &self.subject)
            .finish_non_exhaustive()
    }
}

/// Serializing is deferred to [`EventPublisher::publish`], which can fail, so
/// building a `PendingEvent` can't.
trait ErasedPayload: Send + Sync {
    fn to_json(&self) -> serde_json::Result<Value>;
}

impl<T: Serialize + Send + Sync> ErasedPayload for T {
    fn to_json(&self) -> serde_json::Result<Value> {
        serde_json::to_value(self)
    }
}
