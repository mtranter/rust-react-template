//! Deferred jobs: a module's commands to itself, due at a time it chooses,
//! written into a unit of work as events are and run by the same worker, with
//! the same lease, retries and dead letters. Not events: nothing else
//! subscribes, and the module may reschedule or cancel one by its key.

use crate::Tx;
use app_error::{AppError, AppResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// A command a module defers to its own handler ([`Run`]). Its payload is
/// itself, so it only gains fields, each `#[serde(default)]`, as an event's
/// entity does: a job deferred by the last build runs in the next.
pub trait DeferredJob: Serialize + DeserializeOwned + Send + 'static {
    /// `<module>.<what it does>`, stable: the queue its jobs wait in, so
    /// renaming it strands every pending one.
    const NAME: &'static str;
}

/// Runs a [`DeferredJob`] in the transaction that marks it done, as
/// [`Handle`](crate::Handle) does an event: at least once, so it must be safe
/// to run again, and an error rolls its writes back for a retry.
#[async_trait]
pub trait Run<J: DeferredJob>: Send + Sync {
    async fn run(&self, tx: &mut Tx, job: J) -> AppResult<()>;
}

/// Writes deferred jobs into the caller's unit of work, so one exists if and
/// only if the change that defers it commits. Services hold an
/// `Arc<dyn DeferredJobs>` beside their publisher and call
/// [`defer`](trait.DeferredJobs.html#method.defer) and
/// [`cancel`](trait.DeferredJobs.html#method.cancel).
#[async_trait]
pub trait DeferredJobs: Send + Sync {
    /// Due at `job.run_at`; replaces the pending job of the same type and key.
    async fn write(&self, tx: &mut Tx, job: PendingJob) -> AppResult<()>;

    /// Removes the pending job of type `name` and `key`, if there is one.
    async fn remove(&self, tx: &mut Tx, name: &'static str, key: String) -> AppResult<()>;
}

impl dyn DeferredJobs {
    /// Runs `job` once `at` has passed by the worker's clock. Deferring again
    /// with the same `key` before it runs replaces its time and payload
    /// rather than adding a second job: a reschedule.
    pub async fn defer<J: DeferredJob>(
        &self,
        tx: &mut Tx,
        key: impl Into<String>,
        at: DateTime<Utc>,
        job: &J,
    ) -> AppResult<()> {
        let job = PendingJob::of(key, at, job);
        self.write(tx, job).await
    }

    /// Removes the pending `J` named `key`: `Ok` too when it already ran or
    /// never existed. A dead letter stays, to be seen.
    pub async fn cancel<J: DeferredJob>(
        &self,
        tx: &mut Tx,
        key: impl Into<String>,
    ) -> AppResult<()> {
        self.remove(tx, J::NAME, key.into()).await
    }
}

/// A job to write, its type erased so every module's share one table.
#[derive(Debug)]
pub struct PendingJob {
    pub name: &'static str,
    pub key: String,
    pub run_at: DateTime<Utc>,
    /// Serialized here, but failing only when written, as an event's data does.
    payload: serde_json::Result<Value>,
}

impl PendingJob {
    pub fn of<J: DeferredJob>(key: impl Into<String>, run_at: DateTime<Utc>, job: &J) -> Self {
        Self {
            name: J::NAME,
            key: key.into(),
            run_at,
            payload: serde_json::to_value(job),
        }
    }

    pub fn payload(self) -> AppResult<Value> {
        self.payload.map_err(AppError::internal)
    }
}
