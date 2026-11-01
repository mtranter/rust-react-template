//! Events, jobs and their handlers (spec §4). A [`Tx`] records them; the worker
//! in `app-db` delivers them. Every handler runs at least once per message, so it
//! must be safe to run twice.

use crate::Tx;
use app_error::{AppError, AppResult};
use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::Arc;

/// Attempts before a failing message is dead-lettered.
pub const DEFAULT_MAX_ATTEMPTS: i32 = 5;

/// Something that happened in a module, for other modules to react to. Published
/// in the module's `-api` (`users_api::events::v1::UserDeleted`), as plain values.
/// Stored as JSON, so a version only gains fields, each `#[serde(default)]` so
/// rows written before it still parse; a breaking change is a new `TYPE`.
pub trait Event: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// Stable across releases: `<module>.<event>.v<n>`.
    const TYPE: &'static str;
}

/// Work a module asks its own worker to do later. Private to the module.
pub trait Job: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// The job's queue: stable across releases, `<module>.<job>`.
    const KIND: &'static str;
    const MAX_ATTEMPTS: i32 = DEFAULT_MAX_ATTEMPTS;
}

/// Handles one event or job inside the transaction that marks it done, so its
/// writes commit with that, and an error rolls them back for a retry.
#[async_trait]
pub trait Handle<M: Send + 'static>: Send + Sync {
    async fn handle(&self, tx: &mut Tx, message: M) -> AppResult<()>;
}

/// What a module's worker runs, by queue: each subscription and each job kind
/// is one queue, retried and dead-lettered on its own.
#[derive(Default)]
pub struct Handlers {
    queues: BTreeMap<&'static str, Queue>,
}

struct Queue {
    event_type: Option<&'static str>,
    handler: Arc<dyn Erased>,
}

impl Handlers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribes `handler` to `E`. `queue` names the subscription,
    /// `<module>.<reaction>`; renaming it strands the queue's pending deliveries.
    pub fn on<E: Event>(self, queue: &'static str, handler: impl Handle<E> + 'static) -> Self {
        self.with(queue, Some(E::TYPE), Typed::new(handler))
    }

    pub fn job<J: Job>(self, handler: impl Handle<J> + 'static) -> Self {
        self.with(J::KIND, None, Typed::new(handler))
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
            .filter(move |(_, q)| q.event_type == Some(event_type))
            .map(|(queue, _)| *queue)
    }

    pub async fn handle(&self, queue: &str, tx: &mut Tx, payload: Value) -> AppResult<()> {
        let q = self
            .queues
            .get(queue)
            .ok_or_else(|| AppError::internal(NoHandler(queue.to_owned())))?;
        q.handler.handle(tx, payload).await
    }

    fn with(
        self,
        queue: &'static str,
        event_type: Option<&'static str>,
        handler: impl Erased + 'static,
    ) -> Self {
        self.with_queue(
            queue,
            Queue {
                event_type,
                handler: Arc::new(handler),
            },
        )
    }

    fn with_queue(mut self, queue: &'static str, q: Queue) -> Self {
        assert!(
            self.queues.insert(queue, q).is_none(),
            "two handlers for the queue `{queue}`"
        );
        self
    }
}

/// A message whose queue no handler serves, e.g. one removed since it was sent:
/// it fails like any other, and is dead-lettered once its attempts run out.
#[derive(Debug, thiserror::Error)]
#[error("no handler for the queue `{0}`")]
struct NoHandler(String);

#[async_trait]
trait Erased: Send + Sync {
    async fn handle(&self, tx: &mut Tx, payload: Value) -> AppResult<()>;
}

struct Typed<M, H> {
    handler: H,
    message: PhantomData<fn() -> M>,
}

impl<M, H> Typed<M, H> {
    fn new(handler: H) -> Self {
        Self {
            handler,
            message: PhantomData,
        }
    }
}

#[async_trait]
impl<M, H> Erased for Typed<M, H>
where
    M: DeserializeOwned + Send + 'static,
    H: Handle<M>,
{
    async fn handle(&self, tx: &mut Tx, payload: Value) -> AppResult<()> {
        let message = serde_json::from_value(payload).map_err(AppError::internal)?;
        self.handler.handle(tx, message).await
    }
}
