//! A unit of work (spec §4): everything a use case writes, its aggregates and the
//! events they cause, commits together or not at all. Pure: `app-db` supplies
//! the transaction behind [`Tx`], so `application/` never names the driver.

use app_error::{AppError, AppResult};
use app_events::{Event, PayloadOf};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;

/// Begins units of work. Services hold an `Arc<dyn Transactions>`.
#[async_trait]
pub trait Transactions: Send + Sync {
    async fn begin(&self) -> AppResult<Tx>;
}

/// One open transaction. Repository writes take `&mut Tx`; dropping it without
/// [`commit`](Self::commit), as `?` does on an error, rolls everything back.
pub struct Tx {
    backend: Box<dyn TxBackend>,
    events: Vec<PendingEvent>,
}

/// The database transaction behind a [`Tx`]; implemented only by `app-db`.
#[async_trait]
pub trait TxBackend: Any + Send {
    /// Writes `events` to the outbox, then commits.
    async fn commit(self: Box<Self>, events: Vec<PendingEvent>) -> AppResult<()>;
}

/// An event a unit of work published, serialized, waiting for its commit, which
/// stamps it with an id and time to make its [`EventEnvelope`](app_events::EventEnvelope).
#[derive(Debug, Clone, PartialEq)]
pub struct PendingEvent {
    pub event_type: &'static str,
    pub source: &'static str,
    pub subject: String,
    pub data: Value,
}

impl Tx {
    pub fn new(backend: impl TxBackend) -> Self {
        Self {
            backend: Box::new(backend),
            events: Vec::new(),
        }
    }

    /// Delivered to every subscriber, at least once, if and only if this commits:
    /// `tx.publish::<UserRegistered>(EventPayload::new(user))`.
    pub fn publish<E: Event>(&mut self, payload: PayloadOf<E>) -> AppResult<()> {
        self.events.push(PendingEvent {
            event_type: E::TYPE,
            source: E::SOURCE,
            subject: E::subject(&payload.entity),
            data: serde_json::to_value(&payload).map_err(AppError::internal)?,
        });
        Ok(())
    }

    pub async fn commit(self) -> AppResult<()> {
        self.backend.commit(self.events).await
    }

    /// The concrete transaction, for the db layer (`app_db::conn`).
    pub fn backend<B: TxBackend>(&mut self) -> Option<&mut B> {
        let backend: &mut dyn Any = &mut *self.backend;
        backend.downcast_mut()
    }
}
