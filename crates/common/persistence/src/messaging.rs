//! Subscribers and their registration (ADR 0014). An
//! [`EventPublisher`](crate::EventPublisher) records events; the worker in `app-db`
//! delivers them. Every handler runs at least once per
//! event, so it must be safe to run twice.

use crate::{DeclaredSchedule, DeferredJob, Run, RunScheduled, Schedule, Slot, Tx};
use app_error::{AppError, AppResult};
use app_events::{Subscribable, Topic};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::Arc;

/// Attempts before a failing delivery is dead-lettered.
pub const MAX_ATTEMPTS: i32 = 5;

/// Reacts to one delivery inside the transaction that marks it done, so its
/// writes commit with that, and an error rolls them back for a retry. `S` is
/// one event (`Handle<UserDeleted>`, receiving an `EnvelopeOf<UserDeleted>`) or a
/// whole topic (`Handle<UserTopic>`, receiving a `TopicEnvelope<UserTopic>`
/// whose `event` it matches with no wildcard arm).
#[async_trait]
pub trait Handle<S: Subscribable>: Send + Sync {
    async fn handle(&self, tx: &mut Tx, event: S::Envelope) -> AppResult<()>;
}

/// What a module's worker runs, by queue: each subscription is one queue,
/// retried and dead-lettered on its own. `migrate` registers what they list
/// ([`subscriptions`](Self::subscriptions)); the relay fans events out to what
/// is registered.
#[derive(Default)]
pub struct Handlers {
    queues: BTreeMap<&'static str, Queue>,
    jobs: BTreeMap<&'static str, Arc<dyn ErasedHandler>>,
    schedules: BTreeMap<&'static str, &'static str>,
}

struct Queue {
    topic: &'static str,
    event_types: &'static [&'static str],
    starts: Start,
    handler: Arc<dyn ErasedHandler>,
}

/// Which events a new subscription receives: those relayed after `migrate`
/// first registers it, or also every one its topic's outbox holds then.
/// Fixed at that first registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Latest,
    Earliest,
}

impl Start {
    /// As `messaging.subscription.starts` records it.
    pub fn as_str(self) -> &'static str {
        match self {
            Start::Latest => "latest",
            Start::Earliest => "earliest",
        }
    }
}

/// What a worker may claim from `messaging.job`: a subscription's queue and
/// one of its event types, or a deferred job's type as both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claim {
    pub queue: &'static str,
    pub event_type: &'static str,
}

/// A queue and an event type of the topic it's bound to: one row of
/// `messaging.subscription`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscription {
    pub queue: &'static str,
    pub topic: &'static str,
    pub event_type: &'static str,
    pub starts: Start,
}

impl Handlers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribes `handler` to `S`: one event, or a whole topic, delivered in
    /// order per key. `queue` names the subscription, `<module>.<reaction>`;
    /// renaming it strands the queue's pending deliveries.
    pub fn on<S: Subscribable>(
        self,
        queue: &'static str,
        handler: impl Handle<S> + 'static,
    ) -> Self {
        self.subscribe(queue, Start::Latest, handler)
    }

    /// As [`on`](Self::on), but a new subscription is first delivered every
    /// event of `S` its topic's outbox still holds, oldest first: after
    /// compaction, each key's latest event and the history within the topic's
    /// retention. For a copy of state, applied by version: a reaction to facts
    /// would re-run for history. Live events of a key wait for its backfilled
    /// ones; other keys' don't.
    pub fn on_from_earliest<S: Subscribable>(
        self,
        queue: &'static str,
        handler: impl Handle<S> + 'static,
    ) -> Self {
        self.subscribe(queue, Start::Earliest, handler)
    }

    /// Runs each [`DeferredJob`] `J` once due, as `handler` says: in the
    /// queue `J::NAME`, which no subscription may share. Registered by
    /// nothing: the job's type is its queue.
    pub fn runs<J: DeferredJob>(mut self, handler: impl Run<J> + 'static) -> Self {
        self.assert_free(J::NAME);
        self.jobs
            .insert(J::NAME, Arc::new(JobHandler::<J, _>::new(handler)));
        self
    }

    /// Runs `handler` for each slot of the [`Schedule`] `S` that falls due:
    /// in the queue `S::NAME`, which no subscription or deferred job may
    /// share. `migrate` registers it in `messaging.schedule`.
    pub fn every<S: Schedule>(mut self, handler: impl RunScheduled<S> + 'static) -> Self {
        self.assert_free(S::NAME);
        self.jobs
            .insert(S::NAME, Arc::new(ScheduleHandler::<S, _>::new(handler)));
        self.schedules.insert(S::NAME, S::CRON);
        self
    }

    /// Every schedule these handlers run, by name: what `migrate` registers.
    pub fn schedules(&self) -> impl Iterator<Item = DeclaredSchedule> + '_ {
        self.schedules
            .iter()
            .map(|(&name, &cron)| DeclaredSchedule { name, cron })
    }

    fn subscribe<S: Subscribable>(
        self,
        queue: &'static str,
        starts: Start,
        handler: impl Handle<S> + 'static,
    ) -> Self {
        self.with_queue(
            queue,
            Queue {
                topic: <S::Topic as Topic>::NAME,
                event_types: S::TYPES,
                starts,
                handler: Arc::new(TypedHandler::<S, _>::new(handler)),
            },
        )
    }

    /// Panics on a queue both handle.
    pub fn merge(self, other: Handlers) -> Self {
        let Handlers {
            queues,
            jobs,
            schedules,
        } = other;
        let mut merged = queues
            .into_iter()
            .fold(self, |handlers, (queue, q)| handlers.with_queue(queue, q));
        for (name, handler) in jobs {
            merged.assert_free(name);
            merged.jobs.insert(name, handler);
        }
        merged.schedules.extend(schedules);
        merged
    }

    /// Every queue and type this worker can run: its subscriptions', then its
    /// deferred jobs' and schedules'.
    pub fn claims(&self) -> impl Iterator<Item = Claim> + '_ {
        let subscribed = self.subscriptions().map(|s| Claim {
            queue: s.queue,
            event_type: s.event_type,
        });
        let deferred = self.jobs.keys().map(|&name| Claim {
            queue: name,
            event_type: name,
        });
        subscribed.chain(deferred)
    }

    /// Every `(queue, event type)` these handlers can decode, by queue: what a
    /// worker running them may claim.
    pub fn subscriptions(&self) -> impl Iterator<Item = Subscription> + '_ {
        self.queues.iter().flat_map(|(queue, q)| {
            q.event_types.iter().map(|event_type| Subscription {
                queue,
                topic: q.topic,
                event_type,
                starts: q.starts,
            })
        })
    }

    pub async fn handle(&self, queue: &str, tx: &mut Tx, payload: Value) -> AppResult<()> {
        let handler = self
            .queues
            .get(queue)
            .map(|q| &q.handler)
            .or_else(|| self.jobs.get(queue))
            .ok_or_else(|| AppError::internal(NoHandler(queue.to_owned())))?;
        handler.handle(tx, payload).await
    }

    fn with_queue(mut self, queue: &'static str, q: Queue) -> Self {
        self.assert_free(queue);
        self.queues.insert(queue, q);
        self
    }

    fn assert_free(&self, queue: &str) {
        assert!(
            !self.queues.contains_key(queue) && !self.jobs.contains_key(queue),
            "two handlers for the queue `{queue}`"
        );
    }
}

/// A delivery whose queue no handler serves. A worker claims only the
/// subscriptions its handlers list, so a job no running worker can decode (one
/// for a subscription added in a newer build, mid-deploy) waits for one that
/// can; this guards the claim, and fails the delivery like any other error.
#[derive(Debug, thiserror::Error)]
#[error("no handler for the queue `{0}`")]
struct NoHandler(String);

/// A [`Handle`] with its event type erased, so handlers of different events
/// share one map and take the raw JSON the worker reads from the queue.
#[async_trait]
trait ErasedHandler: Send + Sync {
    async fn handle(&self, tx: &mut Tx, payload: Value) -> AppResult<()>;
}

struct TypedHandler<S, H> {
    handler: H,
    subscribed: PhantomData<fn() -> S>,
}

impl<S, H> TypedHandler<S, H> {
    fn new(handler: H) -> Self {
        Self {
            handler,
            subscribed: PhantomData,
        }
    }
}

#[async_trait]
impl<S: Subscribable, H: Handle<S>> ErasedHandler for TypedHandler<S, H> {
    /// Refuses a delivery of a topic or type the queue didn't register, e.g.
    /// one whose queue name was reused while deliveries were pending.
    async fn handle(&self, tx: &mut Tx, payload: Value) -> AppResult<()> {
        let event = S::decode(payload).map_err(AppError::internal)?;
        self.handler.handle(tx, event).await
    }
}

struct JobHandler<J, H> {
    handler: H,
    runs: PhantomData<fn() -> J>,
}

impl<J, H> JobHandler<J, H> {
    fn new(handler: H) -> Self {
        Self {
            handler,
            runs: PhantomData,
        }
    }
}

#[async_trait]
impl<J: DeferredJob, H: Run<J>> ErasedHandler for JobHandler<J, H> {
    async fn handle(&self, tx: &mut Tx, payload: Value) -> AppResult<()> {
        let job = serde_json::from_value(payload).map_err(AppError::internal)?;
        self.handler.run(tx, job).await
    }
}

struct ScheduleHandler<S, H> {
    handler: H,
    runs: PhantomData<fn() -> S>,
}

impl<S, H> ScheduleHandler<S, H> {
    fn new(handler: H) -> Self {
        Self {
            handler,
            runs: PhantomData,
        }
    }
}

#[async_trait]
impl<S: Schedule, H: RunScheduled<S>> ErasedHandler for ScheduleHandler<S, H> {
    async fn handle(&self, tx: &mut Tx, payload: Value) -> AppResult<()> {
        let Slot { slot } = serde_json::from_value(payload).map_err(AppError::internal)?;
        self.handler.run(tx, slot).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PendingEvent, TxBackend, Version};
    use app_events::{EnvelopeOf, TopicEnvelope};
    use chrono::DateTime;
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct Ping {
        name: String,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct Pong {
        count: u32,
    }

    app_events::topic! {
        pub topic PingTopic("tests.ping.v1") {
            source: "/tests",
            retention: app_events::TimeDelta::days(7),
            entity: Ping,
            key: |ping| ping.name.clone(),
            events: PingEvent, PingEventKind;
            Pinged => Pinged: "tests.pinged",
            /// Carries what `Pinged` does, so only its type tells them apart.
            Poked => Poked: "tests.poked",
        }
    }

    app_events::topic! {
        pub topic PongTopic("tests.pong.v1") {
            source: "/tests",
            retention: app_events::TimeDelta::days(7),
            entity: Pong,
            key: |pong| pong.count.to_string(),
            events: PongEvent, PongEventKind;
            Ponged => Ponged: "tests.ponged",
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

    fn write(tx: &mut Tx, line: String) -> AppResult<()> {
        tx.backend::<FakeTx>().expect("a FakeTx").written.push(line);
        Ok(())
    }

    /// Writes `<label>: <subject>` through the delivery's transaction.
    struct Recorder(&'static str);

    #[async_trait]
    impl Handle<Pinged> for Recorder {
        async fn handle(&self, tx: &mut Tx, event: EnvelopeOf<Pinged>) -> AppResult<()> {
            write(tx, format!("{}: {}", self.0, event.data.entity.name))
        }
    }

    #[async_trait]
    impl Handle<Ponged> for Recorder {
        async fn handle(&self, tx: &mut Tx, event: EnvelopeOf<Ponged>) -> AppResult<()> {
            write(tx, format!("{}: {}", self.0, event.data.entity.count))
        }
    }

    #[async_trait]
    impl Handle<PingTopic> for Recorder {
        async fn handle(&self, tx: &mut Tx, event: TopicEnvelope<PingTopic>) -> AppResult<()> {
            let what = match event.event {
                PingEvent::Pinged => "pinged",
                PingEvent::Poked => "poked",
            };
            write(tx, format!("{}: {what} {}", self.0, event.entity.name))
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
    fn delivery<T: Topic>(state: &T::Entity, event: T::Event) -> Value {
        let epoch = DateTime::from_timestamp(0, 0).unwrap();
        let envelope = PendingEvent::on::<T>(Version::FIRST, state, event)
            .envelope(Uuid::nil(), epoch)
            .unwrap();
        serde_json::to_value(envelope).unwrap()
    }

    fn ping(name: &str) -> Value {
        delivery::<PingTopic>(&Ping { name: name.into() }, PingEvent::Pinged)
    }

    fn poke(name: &str) -> Value {
        delivery::<PingTopic>(&Ping { name: name.into() }, PingEvent::Poked)
    }

    fn pong(count: u32) -> Value {
        delivery::<PongTopic>(&Pong { count }, PongEvent::Ponged)
    }

    fn subscription(
        queue: &'static str,
        topic: &'static str,
        event_type: &'static str,
    ) -> Subscription {
        Subscription {
            queue,
            topic,
            event_type,
            starts: Start::Latest,
        }
    }

    #[test]
    fn handlers_list_what_each_queue_is_delivered() {
        let handlers = Handlers::new()
            .on::<Pinged>("b.on_ping", Recorder("b"))
            .on::<Ponged>("a.on_pong", Recorder("a"))
            .on::<PingTopic>("c.on_pings", Recorder("c"));

        assert_eq!(
            handlers.subscriptions().collect::<Vec<_>>(),
            [
                subscription("a.on_pong", "tests.pong.v1", "tests.ponged"),
                subscription("b.on_ping", "tests.ping.v1", "tests.pinged"),
                subscription("c.on_pings", "tests.ping.v1", "tests.pinged"),
                subscription("c.on_pings", "tests.ping.v1", "tests.poked"),
            ]
        );
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
    async fn a_whole_topic_handler_gets_each_of_its_events() {
        let handlers = Handlers::new().on::<PingTopic>("a.on_pings", Recorder("a"));
        let mut tx = Tx::new(FakeTx::default());

        for delivery in [ping("alice"), poke("bob")] {
            handlers
                .handle("a.on_pings", &mut tx, delivery)
                .await
                .unwrap();
        }

        assert_eq!(written(&mut tx), ["a: pinged alice", "a: poked bob"]);
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
    async fn a_delivery_of_another_topic_fails_without_running_the_handler() {
        let handlers = Handlers::new()
            .on::<Pinged>("a.on_ping", Recorder("a"))
            .on::<PingTopic>("a.on_pings", Recorder("a"));
        let mut tx = Tx::new(FakeTx::default());

        for queue in ["a.on_ping", "a.on_pings"] {
            let result = handlers.handle(queue, &mut tx, pong(3)).await;
            assert!(matches!(result, Err(AppError::Internal(_))));
        }
        assert!(written(&mut tx).is_empty());
    }

    #[tokio::test]
    async fn a_delivery_of_another_event_of_the_topic_fails_without_running_the_handler() {
        let handlers = Handlers::new().on::<Pinged>("a.on_ping", Recorder("a"));
        let mut tx = Tx::new(FakeTx::default());

        let result = handlers.handle("a.on_ping", &mut tx, poke("alice")).await;

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

    #[derive(Debug, Serialize, Deserialize)]
    struct Remind {
        who: String,
    }

    impl DeferredJob for Remind {
        const NAME: &'static str = "a.remind";
    }

    #[async_trait]
    impl Run<Remind> for Recorder {
        async fn run(&self, tx: &mut Tx, job: Remind) -> AppResult<()> {
            write(tx, format!("{}: remind {}", self.0, job.who))
        }
    }

    #[test]
    fn a_worker_claims_its_subscriptions_and_its_deferred_jobs() {
        let handlers = Handlers::new()
            .on::<Pinged>("a.on_ping", Recorder("a"))
            .runs::<Remind>(Recorder("a"));

        assert_eq!(
            handlers.claims().collect::<Vec<_>>(),
            [
                Claim {
                    queue: "a.on_ping",
                    event_type: "tests.pinged"
                },
                Claim {
                    queue: "a.remind",
                    event_type: "a.remind"
                },
            ]
        );
        assert_eq!(handlers.subscriptions().count(), 1);
    }

    #[tokio::test]
    async fn a_deferred_job_runs_its_handler_with_its_payload() {
        let handlers = Handlers::new()
            .on::<Pinged>("a.on_ping", Recorder("a"))
            .merge(Handlers::new().runs::<Remind>(Recorder("b")));
        let mut tx = Tx::new(FakeTx::default());

        handlers
            .handle("a.remind", &mut tx, serde_json::json!({ "who": "alice" }))
            .await
            .unwrap();

        assert_eq!(written(&mut tx), ["b: remind alice"]);
    }

    #[tokio::test]
    async fn a_deferred_job_it_cannot_decode_fails_without_running_the_handler() {
        let handlers = Handlers::new().runs::<Remind>(Recorder("a"));
        let mut tx = Tx::new(FakeTx::default());

        let result = handlers.handle("a.remind", &mut tx, ping("alice")).await;

        assert!(matches!(result, Err(AppError::Internal(_))));
        assert!(written(&mut tx).is_empty());
    }

    struct Nightly;

    impl Schedule for Nightly {
        const NAME: &'static str = "a.nightly";
        const CRON: &'static str = "0 3 * * *";
    }

    #[async_trait]
    impl RunScheduled<Nightly> for Recorder {
        async fn run(&self, tx: &mut Tx, slot: DateTime<chrono::Utc>) -> AppResult<()> {
            write(tx, format!("{}: nightly {}", self.0, slot.to_rfc3339()))
        }
    }

    #[test]
    fn a_worker_claims_its_schedules_runs_and_migrate_registers_them() {
        let handlers = Handlers::new()
            .on::<Pinged>("a.on_ping", Recorder("a"))
            .merge(Handlers::new().every::<Nightly>(Recorder("b")));

        assert_eq!(
            handlers.schedules().collect::<Vec<_>>(),
            [DeclaredSchedule {
                name: "a.nightly",
                cron: "0 3 * * *"
            }]
        );
        assert!(handlers.claims().any(|c| c
            == Claim {
                queue: "a.nightly",
                event_type: "a.nightly"
            }));
        assert_eq!(handlers.subscriptions().count(), 1);
    }

    #[tokio::test]
    async fn a_scheduled_run_runs_its_handler_for_its_slot() {
        let handlers = Handlers::new().every::<Nightly>(Recorder("a"));
        let mut tx = Tx::new(FakeTx::default());

        handlers
            .handle(
                "a.nightly",
                &mut tx,
                serde_json::json!({ "slot": "2025-01-01T03:00:00Z" }),
            )
            .await
            .unwrap();

        assert_eq!(written(&mut tx), ["a: nightly 2025-01-01T03:00:00+00:00"]);
    }

    #[test]
    #[should_panic(expected = "two handlers for the queue `a.nightly`")]
    fn a_schedule_cannot_share_a_queue() {
        let _ = Handlers::new()
            .on::<Pinged>("a.nightly", Recorder("a"))
            .merge(Handlers::new().every::<Nightly>(Recorder("b")));
    }

    #[test]
    #[should_panic(expected = "two handlers for the queue `a.remind`")]
    fn a_deferred_job_cannot_share_a_subscriptions_queue() {
        let _ = Handlers::new()
            .on::<Pinged>("a.remind", Recorder("a"))
            .merge(Handlers::new().runs::<Remind>(Recorder("b")));
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
