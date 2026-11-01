use app_db::{PgEventPublisher, PgTransactions, Worker};
use app_domain::{FixedClock, seed_time};
use app_error::AppResult;
use app_events::{EnvelopeOf, Event, EventPayload, NoContext};
use app_persistence::{EventPublisher, Handle, Handlers, PendingEvent, Transactions, Tx};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ping {
    pub name: String,
}

pub struct Pinged;

impl Event for Pinged {
    const TYPE: &'static str = "tests.pinged.v1";
    const SOURCE: &'static str = "/tests";
    type Entity = Ping;
    type Context = NoContext;

    fn subject(ping: &Ping) -> String {
        ping.name.clone()
    }
}

/// What every recorder has handled, `<queue>: <subject>`, in order.
#[derive(Clone, Default)]
pub struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    pub fn handled(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

pub struct Recorder {
    queue: &'static str,
    log: Log,
}

#[async_trait]
impl Handle<Pinged> for Recorder {
    async fn handle(&self, _: &mut Tx, event: EnvelopeOf<Pinged>) -> AppResult<()> {
        let line = format!("{}: {}", self.queue, event.data.entity.name);
        self.log.0.lock().unwrap().push(line);
        Ok(())
    }
}

/// One database, its clock, and what any of its workers' handlers have run.
pub struct Messaging {
    pub pool: PgPool,
    clock: Arc<FixedClock>,
    pub log: Log,
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
        }
    }

    /// A worker built with handlers for `queues`, each subscribed to `Pinged`.
    pub fn worker(&self, queues: &[&'static str]) -> Worker {
        let handlers = queues.iter().fold(Handlers::new(), |handlers, &queue| {
            handlers.on::<Pinged>(
                queue,
                Recorder {
                    queue,
                    log: self.log.clone(),
                },
            )
        });
        Worker::new(self.pool.clone(), self.clock.clone(), handlers)
    }

    pub async fn ping(&self, name: &str) {
        let transactions = PgTransactions::new(self.pool.clone());
        let mut tx = transactions.begin().await.expect("begin");
        let event = PendingEvent::of::<Pinged>(EventPayload::new(Ping { name: name.into() }));
        PgEventPublisher::new(self.clock.clone())
            .publish(&mut tx, event)
            .await
            .expect("publish");
        tx.commit().await.expect("commit");
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
}

pub async fn run(worker: &Worker) {
    worker.run_until_idle().await.expect("the worker runs");
}
