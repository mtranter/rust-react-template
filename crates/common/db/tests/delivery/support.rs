use app_db::{
    PgDeferredJobs, PgEventPublisher, PgTransactions, ScheduleError, SubscriptionError, Worker,
};
use app_domain::{FixedClock, seed_time};
use app_error::{AppError, AppResult};
use app_events::{EnvelopeOf, PublishedTopic, TopicEnvelope};
use app_persistence::{
    DeferredJobs, EventPublisher, Handle, Handlers, PendingEvent, Transactions, Tx, Version,
};
use app_test_db::Template;
use async_trait::async_trait;
use chrono::TimeDelta;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::layer::SubscriberExt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ping {
    pub name: String,
}

app_events::topic! {
    pub topic PingTopic("tests.ping.v1") {
        source: "/tests",
        retention: TimeDelta::days(7),
        entity: Ping,
        key: |ping| ping.name.clone(),
        events: PingEvent, PingEventKind;
        Pinged => Pinged: "tests.pinged",
        Ponged => Ponged: "tests.ponged",
        /// The ping is gone, as it was last: a deletion.
        Dropped => Dropped: "tests.dropped",
    }
}

app_events::topic! {
    /// Another topic of the same entity.
    pub topic PokeTopic("tests.poke.v1") {
        source: "/tests",
        retention: TimeDelta::days(1),
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

/// `name` dropped, at `version`.
pub fn drop(name: &str, version: i64) -> PendingEvent {
    happened(name, version, PingEvent::Dropped)
}

fn happened(name: &str, version: i64, event: PingEvent) -> PendingEvent {
    PendingEvent::on::<PingTopic>(
        Version::from_i64(version),
        &Ping { name: name.into() },
        event,
    )
}

/// `name` poked, at `version`.
pub fn poke(name: &str, version: i64) -> PendingEvent {
    PendingEvent::on::<PokeTopic>(
        Version::from_i64(version),
        &Ping { name: name.into() },
        PokeEvent::Poked,
    )
}

/// What every recorder has handled, `<queue>: <event> <key> v<version>`, in order.
#[derive(Clone, Default)]
pub struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    pub fn handled(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }

    pub fn push(&self, line: String) {
        self.0.lock().unwrap().push(line);
    }
}

/// How many more times handling each `(key, version)` fails.
#[derive(Clone, Default)]
struct Faults(Arc<Mutex<HashMap<(String, i64), u32>>>);

/// Which `(key, version)`s never finish handling, and how many times one began.
#[derive(Clone, Default)]
struct Hangs {
    keys: Arc<Mutex<HashSet<(String, i64)>>>,
    begun: Arc<AtomicUsize>,
}

pub struct Recorder {
    queue: &'static str,
    log: Log,
    faults: Faults,
    hangs: Hangs,
}

impl Recorder {
    async fn record(&self, event: &str, key: &str, version: i64) -> AppResult<()> {
        let hangs = self
            .hangs
            .keys
            .lock()
            .unwrap()
            .contains(&(key.into(), version));
        if hangs {
            self.hangs.begun.fetch_add(1, Ordering::SeqCst);
            std::future::pending::<()>().await;
        }
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
            .await
    }
}

#[async_trait]
impl Handle<Poked> for Recorder {
    async fn handle(&self, _: &mut Tx, event: EnvelopeOf<Poked>) -> AppResult<()> {
        self.record("poked", &event.partition_key, event.entity_version)
            .await
    }
}

#[async_trait]
impl Handle<PingTopic> for Recorder {
    async fn handle(&self, _: &mut Tx, event: TopicEnvelope<PingTopic>) -> AppResult<()> {
        let what = match event.event {
            PingEvent::Pinged => "pinged",
            PingEvent::Ponged => "ponged",
            PingEvent::Dropped => "dropped",
        };
        self.record(what, &event.key, event.version).await
    }
}

/// One database, its clock, and what any of its workers' handlers have run.
pub struct Messaging {
    pub pool: PgPool,
    clock: Arc<FixedClock>,
    pub log: Log,
    faults: Faults,
    hangs: Hangs,
}

impl Messaging {
    pub async fn new(pool: PgPool) -> Self {
        Self {
            pool,
            clock: Arc::new(FixedClock::new(seed_time(0))),
            log: Log::default(),
            faults: Faults::default(),
            hangs: Hangs::default(),
        }
    }

    pub fn recorder(&self, queue: &'static str) -> Recorder {
        Recorder {
            queue,
            log: self.log.clone(),
            faults: self.faults.clone(),
            hangs: self.hangs.clone(),
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

    /// Handling `key` at `version` never returns, in any queue.
    pub fn hangs(&self, key: &str, version: i64) {
        self.hangs
            .keys
            .lock()
            .unwrap()
            .insert((key.into(), version));
    }

    /// Runs `worker` until one of its handlers hangs, then drops it
    /// mid-handler, as a worker that is killed or stopped would be.
    pub async fn run_until_a_handler_hangs(&self, worker: &Worker) {
        let begun = self.hangs.begun.load(Ordering::SeqCst);
        let hung = async {
            while self.hangs.begun.load(Ordering::SeqCst) == begun {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        };
        tokio::select! {
            _ = worker.run_until_idle() => panic!("the worker went idle without a handler hanging"),
            () = hung => {}
        }
    }

    pub fn later(&self, by: TimeDelta) {
        self.clock.advance(by);
    }

    pub fn now(&self) -> chrono::DateTime<chrono::Utc> {
        use app_domain::Clock;
        self.clock.now()
    }

    pub fn deferred_jobs(&self) -> Arc<dyn DeferredJobs> {
        Arc::new(PgDeferredJobs::new(self.clock.clone()))
    }

    pub async fn begin(&self) -> Tx {
        PgTransactions::new(self.pool.clone())
            .begin()
            .await
            .expect("begin")
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
        app_db::register_subscriptions(&self.pool, self.clock.as_ref(), handlers).await
    }

    /// As `migrate` does for the schedules `handlers` declare.
    pub async fn register_schedules(&self, handlers: &Handlers) -> Result<(), ScheduleError> {
        app_db::register_schedules(&self.pool, self.clock.as_ref(), handlers).await
    }

    /// When the schedule `name` is next due, as registered.
    pub async fn next_at(&self, name: &str) -> chrono::DateTime<chrono::Utc> {
        sqlx::query_scalar("SELECT next_at FROM messaging.schedule WHERE name = $1")
            .bind(name)
            .fetch_one(&self.pool)
            .await
            .expect("a registered schedule")
    }

    /// As `migrate` does for a build publishing `topics`.
    pub async fn register_topics(&self, topics: &[PublishedTopic]) {
        app_db::register_topics(&self.pool, topics)
            .await
            .expect("register the topics");
    }

    /// The migration ADR 0014 documents.
    pub async fn retire(&self, queue: &str) {
        sqlx::query("SELECT messaging.retire($1)")
            .bind(queue)
            .execute(&self.pool)
            .await
            .expect("retire the subscription");
    }

    /// [`retire`](Self::retire) in a transaction left open: it takes effect
    /// once the caller commits it.
    pub async fn retiring(&self, queue: &str) -> sqlx::Transaction<'static, sqlx::Postgres> {
        let mut tx = self.pool.begin().await.expect("begin");
        sqlx::query("SELECT messaging.retire($1)")
            .bind(queue)
            .execute(&mut *tx)
            .await
            .expect("retire the subscription");
        tx
    }

    /// Compaction fails until [`mend_compaction`](Self::mend_compaction),
    /// while the relay and jobs run on.
    pub async fn break_compaction(&self) {
        sqlx::query("ALTER TABLE messaging.topic RENAME TO topic_broken")
            .execute(&self.pool)
            .await
            .expect("break compaction");
    }

    pub async fn mend_compaction(&self) {
        sqlx::query("ALTER TABLE messaging.topic_broken RENAME TO topic")
            .execute(&self.pool)
            .await
            .expect("mend compaction");
    }

    /// Reruns every dead letter, as `DeadLetter` documents.
    pub async fn rerun_dead_letters(&self) {
        sqlx::query(
            "UPDATE messaging.job SET dead_at = NULL, attempts = 0 WHERE dead_at IS NOT NULL",
        )
        .execute(&self.pool)
        .await
        .expect("rerun the dead letters");
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

    /// Every event the outbox still holds, oldest first:
    /// `<event type> <key> v<version>`.
    pub async fn outbox(&self) -> Vec<String> {
        let payloads: Vec<serde_json::Value> =
            sqlx::query_scalar("SELECT payload FROM messaging.outbox ORDER BY seq")
                .fetch_all(&self.pool)
                .await
                .expect("read the outbox");
        payloads
            .iter()
            .map(|e| {
                format!(
                    "{} {} v{}",
                    str(&e["type"]),
                    str(&e["partitionkey"]),
                    e["entityversion"]
                )
            })
            .collect()
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

fn str(value: &serde_json::Value) -> &str {
    value.as_str().expect("a string")
}

pub async fn run(worker: &Worker) {
    worker.run_until_idle().await.expect("the worker runs");
}

/// Every test's database starts as a copy of this: messaging migrated.
pub const TEMPLATE: Template = Template::new("delivery", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &[])
            .await
            .expect("migrate messaging");
    })
});

/// The events logged on this thread at one level from
/// [`capture`](Self::capture) until the guard drops, each as
/// `<message> <field>=<value>…`. A test runs on one thread, so another test's
/// logging never shows here.
#[derive(Clone)]
pub struct LoggedErrors {
    lines: Arc<Mutex<Vec<String>>>,
    level: tracing::Level,
}

impl LoggedErrors {
    /// The errors.
    pub fn capture() -> (Self, tracing::subscriber::DefaultGuard) {
        Self::capture_at(tracing::Level::ERROR)
    }

    pub fn capture_at(level: tracing::Level) -> (Self, tracing::subscriber::DefaultGuard) {
        let logged = Self {
            lines: Arc::default(),
            level,
        };
        let subscriber = tracing_subscriber::registry().with(logged.clone());
        let guard = tracing::subscriber::set_default(subscriber);
        // Another test's thread, with no subscriber, may have cached a
        // callsite as never enabled while this one was being set.
        tracing::callsite::rebuild_interest_cache();
        (logged, guard)
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for LoggedErrors {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        if *event.metadata().level() != self.level {
            return;
        }
        let mut line = Line::default();
        event.record(&mut line);
        self.lines
            .lock()
            .unwrap()
            .push(format!("{}{}", line.message, line.fields));
    }
}

#[derive(Default)]
struct Line {
    message: String,
    fields: String,
}

impl tracing::field::Visit for Line {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.fields += &format!(" {}={value:?}", field.name());
        }
    }
}
