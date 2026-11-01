//! Subscribers and their registration (spec §4). An
//! [`EventPublisher`](crate::EventPublisher) records events; the worker in `app-db`
//! delivers them. Every handler runs at least once per
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

/// A delivery of another event than its queue's, e.g. one whose queue name was
/// reused for a different subscription while deliveries were pending.
#[derive(Debug, thiserror::Error)]
#[error("the queue of `{expected}` was delivered a `{delivered}`")]
struct WrongEvent {
    expected: &'static str,
    delivered: String,
}

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
        let event: EnvelopeOf<E> = serde_json::from_value(payload).map_err(AppError::internal)?;
        if event.event_type != E::TYPE {
            return Err(AppError::internal(WrongEvent {
                expected: E::TYPE,
                delivered: event.event_type,
            }));
        }
        self.handler.handle(tx, event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TxBackend;
    use app_events::{EventEnvelope, EventPayload, NoContext, PayloadOf};
    use chrono::DateTime;
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    #[derive(Debug, Serialize, Deserialize)]
    struct Ping {
        name: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Pong {
        count: u32,
    }

    struct Pinged;

    impl Event for Pinged {
        const TYPE: &'static str = "tests.pinged.v1";
        const SOURCE: &'static str = "/tests";
        type Entity = Ping;
        type Context = NoContext;

        fn subject(ping: &Ping) -> String {
            ping.name.clone()
        }
    }

    /// Carries what `Pinged` does, so only its type tells them apart.
    struct Poked;

    impl Event for Poked {
        const TYPE: &'static str = "tests.poked.v1";
        const SOURCE: &'static str = "/tests";
        type Entity = Ping;
        type Context = NoContext;

        fn subject(ping: &Ping) -> String {
            ping.name.clone()
        }
    }

    struct Ponged;

    impl Event for Ponged {
        const TYPE: &'static str = "tests.ponged.v1";
        const SOURCE: &'static str = "/tests";
        type Entity = Pong;
        type Context = NoContext;

        fn subject(pong: &Pong) -> String {
            pong.count.to_string()
        }
    }

    /// Records what handlers wrote through it; committing is a no-op.
    #[derive(Default)]
    struct FakeTx {
        written: Vec<String>,
    }

    #[async_trait]
    impl TxBackend for FakeTx {
        async fn commit(self: Box<Self>) -> AppResult<()> {
            Ok(())
        }
    }

    fn written(tx: &mut Tx) -> Vec<String> {
        tx.backend::<FakeTx>().expect("a FakeTx").written.clone()
    }

    /// Writes `<label>: <subject>` through the delivery's transaction.
    struct Recorder(&'static str);

    #[async_trait]
    impl Handle<Pinged> for Recorder {
        async fn handle(&self, tx: &mut Tx, event: EnvelopeOf<Pinged>) -> AppResult<()> {
            let line = format!("{}: {}", self.0, event.data.entity.name);
            tx.backend::<FakeTx>().expect("a FakeTx").written.push(line);
            Ok(())
        }
    }

    #[async_trait]
    impl Handle<Ponged> for Recorder {
        async fn handle(&self, tx: &mut Tx, event: EnvelopeOf<Ponged>) -> AppResult<()> {
            let line = format!("{}: {}", self.0, event.data.entity.count);
            tx.backend::<FakeTx>().expect("a FakeTx").written.push(line);
            Ok(())
        }
    }

    struct Refusing;

    #[async_trait]
    impl Handle<Pinged> for Refusing {
        async fn handle(&self, _: &mut Tx, _: EnvelopeOf<Pinged>) -> AppResult<()> {
            Err(AppError::conflict("not now"))
        }
    }

    /// As the worker reads it from the queue.
    fn delivery<E: Event>(payload: PayloadOf<E>) -> Value {
        let subject = E::subject(&payload.entity);
        let epoch = DateTime::from_timestamp(0, 0).unwrap();
        let envelope = EventEnvelope::new(Uuid::nil(), epoch, E::SOURCE, E::TYPE, subject, payload);
        serde_json::to_value(envelope).unwrap()
    }

    fn ping(name: &str) -> Value {
        delivery::<Pinged>(EventPayload::new(Ping { name: name.into() }))
    }

    fn pong(count: u32) -> Value {
        delivery::<Ponged>(EventPayload::new(Pong { count }))
    }

    #[test]
    fn an_events_subscribers_are_every_queue_subscribed_to_it() {
        let handlers = Handlers::new()
            .on::<Pinged>("b.on_ping", Recorder("b"))
            .on::<Ponged>("a.on_pong", Recorder("a"))
            .on::<Pinged>("a.on_ping", Recorder("a"));

        assert_eq!(
            handlers.subscribers(Pinged::TYPE).collect::<Vec<_>>(),
            ["a.on_ping", "b.on_ping"]
        );
        assert_eq!(
            handlers.subscribers(Ponged::TYPE).collect::<Vec<_>>(),
            ["a.on_pong"]
        );
    }

    #[test]
    fn an_event_nobody_subscribes_to_has_no_queues() {
        let handlers = Handlers::new().on::<Pinged>("a.on_ping", Recorder("a"));

        assert_eq!(handlers.subscribers(Ponged::TYPE).count(), 0);
    }

    #[tokio::test]
    async fn a_delivery_runs_its_queues_handler_in_the_deliverys_transaction() {
        let handlers = Handlers::new()
            .on::<Pinged>("a.on_ping", Recorder("a"))
            .on::<Pinged>("b.on_ping", Recorder("b"));
        let mut tx = Tx::new(FakeTx::default());

        handlers
            .handle("b.on_ping", &mut tx, ping("alice"))
            .await
            .unwrap();

        assert_eq!(written(&mut tx), ["b: alice"]);
    }

    #[tokio::test]
    async fn a_handlers_error_fails_the_delivery() {
        let handlers = Handlers::new().on::<Pinged>("a.on_ping", Refusing);
        let mut tx = Tx::new(FakeTx::default());

        let result = handlers.handle("a.on_ping", &mut tx, ping("alice")).await;

        assert!(matches!(result, Err(AppError::Conflict(_))));
    }

    #[tokio::test]
    async fn a_delivery_to_a_queue_nobody_serves_fails() {
        let handlers = Handlers::new().on::<Pinged>("a.on_ping", Recorder("a"));
        let mut tx = Tx::new(FakeTx::default());

        let result = handlers.handle("a.removed", &mut tx, ping("alice")).await;

        assert!(matches!(result, Err(AppError::Internal(_))));
        assert!(written(&mut tx).is_empty());
    }

    #[tokio::test]
    async fn a_delivery_that_isnt_the_queues_event_fails_without_running_the_handler() {
        let handlers = Handlers::new().on::<Pinged>("a.on_ping", Recorder("a"));
        let mut tx = Tx::new(FakeTx::default());

        let result = handlers.handle("a.on_ping", &mut tx, pong(3)).await;

        assert!(matches!(result, Err(AppError::Internal(_))));
        assert!(written(&mut tx).is_empty());
    }

    #[tokio::test]
    async fn a_delivery_of_another_event_of_the_same_shape_fails_without_running_the_handler() {
        let handlers = Handlers::new().on::<Pinged>("a.on_ping", Recorder("a"));
        let mut tx = Tx::new(FakeTx::default());
        let poke = delivery::<Poked>(EventPayload::new(Ping {
            name: "alice".into(),
        }));

        let result = handlers.handle("a.on_ping", &mut tx, poke).await;

        assert!(matches!(result, Err(AppError::Internal(_))));
        assert!(written(&mut tx).is_empty());
    }

    #[tokio::test]
    async fn merged_handlers_serve_every_modules_queues() {
        let handlers = Handlers::new()
            .on::<Pinged>("a.on_ping", Recorder("a"))
            .merge(Handlers::new().on::<Ponged>("b.on_pong", Recorder("b")));
        let mut tx = Tx::new(FakeTx::default());

        handlers
            .handle("a.on_ping", &mut tx, ping("alice"))
            .await
            .unwrap();
        handlers
            .handle("b.on_pong", &mut tx, pong(3))
            .await
            .unwrap();

        assert_eq!(written(&mut tx), ["a: alice", "b: 3"]);
    }

    #[test]
    #[should_panic(expected = "two handlers for the queue `a.on_ping`")]
    fn two_handlers_cannot_share_a_queue() {
        let _ = Handlers::new()
            .on::<Pinged>("a.on_ping", Recorder("a"))
            .on::<Pinged>("a.on_ping", Recorder("b"));
    }

    #[test]
    #[should_panic(expected = "two handlers for the queue `a.on_ping`")]
    fn merging_cannot_give_a_queue_two_handlers() {
        let _ = Handlers::new()
            .on::<Pinged>("a.on_ping", Recorder("a"))
            .merge(Handlers::new().on::<Ponged>("a.on_ping", Recorder("b")));
    }
}
