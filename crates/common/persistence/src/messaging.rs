//! Subscribers and their registration (spec §4). A [`Tx`] records events; the
//! worker in `app-db` delivers them. Every handler runs at least once per
//! event, so it must be safe to run twice.

use crate::Tx;
use app_error::{AppError, AppResult};
use app_events::{EnvelopeOf, Event};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::Arc;

/// Attempts before a failing delivery is dead-lettered.
pub const MAX_ATTEMPTS: i32 = 5;

/// Reacts to one delivery of `E` inside the transaction that marks it done, so
/// its writes commit with that, and an error rolls them back for a retry.
#[async_trait]
pub trait Handle<E: Event>: Send + Sync {
    async fn handle(&self, tx: &mut Tx, event: EnvelopeOf<E>) -> AppResult<()>;
}

/// What a module's worker runs, by queue: each subscription is one queue,
/// retried and dead-lettered on its own.
#[derive(Default)]
pub struct Handlers {
    queues: BTreeMap<&'static str, Queue>,
}

struct Queue {
    event_type: &'static str,
    handler: Arc<dyn ErasedHandler>,
}

impl Handlers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribes `handler` to `E`. `queue` names the subscription,
    /// `<module>.<reaction>`; renaming it strands the queue's pending deliveries.
    pub fn on<E: Event>(self, queue: &'static str, handler: impl Handle<E> + 'static) -> Self {
        self.with_queue(
            queue,
            Queue {
                event_type: E::TYPE,
                handler: Arc::new(TypedHandler::new(handler)),
            },
        )
    }

    /// Panics on a queue both handle.
    pub fn merge(self, other: Handlers) -> Self {
        other
            .queues
            .into_iter()
            .fold(self, |handlers, (queue, q)| handlers.with_queue(queue, q))
    }

    /// The queue of each subscription to `event_type`.
    pub fn subscribers<'a>(
        &'a self,
        event_type: &'a str,
    ) -> impl Iterator<Item = &'static str> + 'a {
        self.queues
            .iter()
            .filter(move |(_, q)| q.event_type == event_type)
            .map(|(queue, _)| *queue)
    }

    pub async fn handle(&self, queue: &str, tx: &mut Tx, payload: Value) -> AppResult<()> {
        let q = self
            .queues
            .get(queue)
            .ok_or_else(|| AppError::internal(NoHandler(queue.to_owned())))?;
        q.handler.handle(tx, payload).await
    }

    fn with_queue(mut self, queue: &'static str, q: Queue) -> Self {
        assert!(
            self.queues.insert(queue, q).is_none(),
            "two handlers for the queue `{queue}`"
        );
        self
    }
}

/// A delivery whose queue no handler serves, e.g. a subscription removed since:
/// it fails like any other, and is dead-lettered once its attempts run out.
#[derive(Debug, thiserror::Error)]
#[error("no handler for the queue `{0}`")]
struct NoHandler(String);

/// A [`Handle`] with its event type erased, so handlers of different events
/// share one map and take the raw JSON the worker reads from the queue.
#[async_trait]
trait ErasedHandler: Send + Sync {
    async fn handle(&self, tx: &mut Tx, payload: Value) -> AppResult<()>;
}

struct TypedHandler<E, H> {
    handler: H,
    event: PhantomData<fn() -> E>,
}

impl<E, H> TypedHandler<E, H> {
    fn new(handler: H) -> Self {
        Self {
            handler,
            event: PhantomData,
        }
    }
}

#[async_trait]
impl<E: Event, H: Handle<E>> ErasedHandler for TypedHandler<E, H> {
    async fn handle(&self, tx: &mut Tx, payload: Value) -> AppResult<()> {
        let event = serde_json::from_value(payload).map_err(AppError::internal)?;
        self.handler.handle(tx, event).await
    }
}
