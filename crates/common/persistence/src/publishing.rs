//! Publishing events (spec §4). Services hold an `Arc<dyn EventPublisher>` beside
//! their repository and publish into the same [`Tx`] they save in, so an event is
//! delivered if and only if the change it announces commits.

use crate::Tx;
use app_error::{AppError, AppResult};
use app_events::{Event, PayloadOf};
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish_announcement(&self, tx: &mut Tx, announcement: Announcement) -> AppResult<()>;
}

/// The typed entry point; it lives on `dyn EventPublisher` because a generic
/// trait method would make the trait unusable as an object.
impl dyn EventPublisher {
    /// Delivered to every subscriber, at least once, if and only if `tx` commits:
    /// `publisher.publish::<UserRegistered>(&mut tx, EventPayload::new(user))`.
    pub async fn publish<E: Event>(&self, tx: &mut Tx, payload: PayloadOf<E>) -> AppResult<()> {
        let announcement = Announcement::of::<E>(payload)?;
        self.publish_announcement(tx, announcement).await
    }
}

/// An event with its type erased and its payload serialized; the publisher stamps
/// it with an id and time to make its [`EventEnvelope`](app_events::EventEnvelope).
#[derive(Debug, Clone, PartialEq)]
pub struct Announcement {
    pub event_type: &'static str,
    pub source: &'static str,
    pub subject: String,
    pub data: Value,
}

impl Announcement {
    pub fn of<E: Event>(payload: PayloadOf<E>) -> AppResult<Self> {
        Ok(Self {
            event_type: E::TYPE,
            source: E::SOURCE,
            subject: E::subject(&payload.entity),
            data: serde_json::to_value(&payload).map_err(AppError::internal)?,
        })
    }
}
