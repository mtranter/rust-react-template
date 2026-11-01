use app_db::{PgEventPublisher, PgTransactions, SubscriptionError, Worker};
use app_domain::{FixedClock, seed_time};
use app_error::{AppError, AppResult};
use app_events::{EnvelopeOf, TopicEnvelope};
use app_persistence::{EventPublisher, Handle, Handlers, PendingEvent, Transactions, Tx, Version};
use async_trait::async_trait;
use chrono::TimeDelta;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ping {
    pub name: String,
}

app_events::topic! {
    pub topic PingTopic("tests.ping.v1") {
        source: "/tests",
        entity: Ping,
        key: |ping| ping.name.clone(),
        events: PingEvent, PingEventKind;
        Pinged => Pinged: "tests.pinged",
        Ponged => Ponged: "tests.ponged",
    }
}

app_events::topic! {
    /// Another topic of the same entity.
    pub topic PokeTopic("tests.poke.v1") {
        source: "/tests",
        entity: Ping,
        key: |ping| ping.name.clone(),
        events: PokeEvent, PokeEventKind;
        Poked => Poked: "tests.poked",
    }
}

/// `name` pinged, at `version`.
pub fn ping(name: &str, version: i64) -> PendingEvent {
    happened(name, version, PingEvent::Pinged)
}

/// `name` ponged, at `version`.
pub fn pong(name: &str, version: i64) -> PendingEvent {
    happened(name, version, PingEvent::Ponged)
}

fn happened(name: &str, version: i64, event: PingEvent) -> PendingEvent {
    PendingEvent::on::<PingTopic>(
        Version::from_i64(version),
        &Ping { name: name.into() },
        event,
    )
}

/// What every recorder has handled, `<queue>: <event> <key> v<version>`, in order.
#[derive(Clone, Default)]
pub struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    pub fn handled(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

/// How many more times handling each `(key, version)` fails.
#[derive(Clone, Default)]
struct Faults(Arc<Mutex<HashMap<(String, i64), u32>>>);

pub struct Recorder {
    queue: &'static str,
    log: Log,
    faults: Faults,
}

impl Recorder {
    fn record(&self, event: &str, key: &str, version: i64) -> AppResult<()> {
        if let Some(left) = self
            .faults
            .0
            .lock()
            .unwrap()
            .get_mut(&(key.into(), version))
            && *left > 0
        {
            *left -= 1;
            return Err(AppError::conflict("not now"));
        }
        let line = format!("{}: {event} {key} v{version}", self.queue);
        self.log.0.lock().unwrap().push(line);
        Ok(())
    }
}

#[async_trait]
impl Handle<Pinged> for Recorder {
    async fn handle(&self, _: &mut Tx, event: EnvelopeOf<Pinged>) -> AppResult<()> {
        self.record("pinged", &event.partition_key, event.entity_version)
    }
}

#[async_trait]
impl Handle<Poked> for Recorder {
    async fn handle(&self, _: &mut Tx, event: EnvelopeOf<Poked>) -> AppResult<()> {
        self.record("poked", &event.partition_key, event.entity_version)
    }
}

#[async_trait]
impl Handle<PingTopic> for Recorder {
    async fn handle(&self, _: &mut Tx, event: TopicEnvelope<PingTopic>) -> AppResult<()> {
        let what = match event.event {
            PingEvent::Pinged => "pinged",
            PingEvent::Ponged => "ponged",
        };
        self.record(what, &event.key, event.version)
    }
}

/// One database, its clock, and what any of its workers' handlers have run.
pub struct Messaging {
    pub pool: PgPool,
    clock: Arc<FixedClock>,
    pub log: Log,
    faults: Faults,
}

impl Messaging {
    pub async fn new(pool: PgPool) -> Self {
        app_db::run_migrations(&pool, &[])
            .await
            .expect("migrate messaging");
        Self {
            pool,
            clock: Arc::new(FixedClock::new(seed_time(0))),
            log: Log::default(),
            faults: Faults::default(),
        }
    }

    pub fn recorder(&self, queue: &'static str) -> Recorder {
        Recorder {
            queue,
            log: self.log.clone(),
            faults: self.faults.clone(),
        }
    }

    /// Handling `key` at `version` fails the next `times` times, in any queue.
    pub fn fails(&self, key: &str, version: i64, times: u32) {
        self.faults
            .0
            .lock()
            .unwrap()
            .insert((key.into(), version), times);
    }

    pub fn later(&self, by: TimeDelta) {
        self.clock.advance(by);
    }

    /// A build's handlers: one for each of `queues`, subscribed to `Pinged`.
    pub fn handlers(&self, queues: &[&'static str]) -> Handlers {
        queues.iter().fold(Handlers::new(), |handlers, &queue| {
            handlers.on::<Pinged>(queue, self.recorder(queue))
        })
    }

    /// A worker of the build with handlers for `queues`.
    pub fn worker(&self, queues: &[&'static str]) -> Worker {
        self.worker_of(self.handlers(queues))
    }

    pub fn worker_of(&self, handlers: Handlers) -> Worker {
        self.worker_with(self.pool.clone(), handlers)
    }

    /// A worker with no handlers, on its own `pool`: a relay.
    pub fn worker_on(&self, pool: PgPool) -> Worker {
        self.worker_with(pool, Handlers::new())
    }

    fn worker_with(&self, pool: PgPool, handlers: Handlers) -> Worker {
        Worker::new(pool, self.clock.clone(), handlers)
    }

    /// As `migrate` does for the build with handlers for `queues`.
    pub async fn migrate(&self, queues: &[&'static str]) {
        self.register(&self.handlers(queues))
            .await
            .expect("register the subscriptions");
    }

    pub async fn register(&self, handlers: &Handlers) -> Result<(), SubscriptionError> {
        app_db::register_subscriptions(&self.pool, handlers).await
    }

    /// The migration spec §4 documents.
    pub async fn retire(&self, queue: &str) {
        sqlx::query("UPDATE messaging.subscription SET retired_at = now() WHERE queue = $1")
            .bind(queue)
            .execute(&self.pool)
            .await
            .expect("retire the subscription");
        sqlx::query(
            "UPDATE messaging.job SET dead_at = now(), last_error = 'subscription retired' \
             WHERE queue = $1 AND dead_at IS NULL",
        )
        .bind(queue)
        .execute(&self.pool)
        .await
        .expect("dead-letter its jobs");
    }

    /// `name` pinged, at its first version.
    pub async fn ping(&self, name: &str) {
        self.publish(vec![ping(name, 1)]).await.expect("publish");
    }

    /// Publishes `events` in one unit of work.
    pub async fn publish(&self, events: Vec<PendingEvent>) -> AppResult<()> {
        let transactions = PgTransactions::new(self.pool.clone());
        let mut tx = transactions.begin().await?;
        let publisher = PgEventPublisher::new(self.clock.clone());
        for event in events {
            publisher.publish(&mut tx, event).await?;
        }
        tx.commit().await
    }

    /// Every job still to run or dead, oldest first.
    pub async fn jobs(&self) -> Vec<Job> {
        sqlx::query_as("SELECT queue, attempts, last_error FROM messaging.job ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .expect("read the jobs")
    }
}

#[derive(Debug, PartialEq, sqlx::FromRow)]
pub struct Job {
    pub queue: String,
    pub attempts: i32,
    pub last_error: Option<String>,
}

impl Job {
    pub fn untouched(queue: &str) -> Self {
        Self {
            queue: queue.into(),
            attempts: 0,
            last_error: None,
        }
    }

    pub fn retired(queue: &str) -> Self {
        Self {
            last_error: Some("subscription retired".into()),
            ..Self::untouched(queue)
        }
    }
}

pub async fn run(worker: &Worker) {
    worker.run_until_idle().await.expect("the worker runs");
}
