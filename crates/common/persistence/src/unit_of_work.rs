//! A unit of work (spec §4): everything a use case writes, its aggregates and the
//! events and jobs they cause, commits together or not at all. Pure: `app-db`
//! supplies the transaction behind [`Tx`], so `application/` never names the driver.

use crate::messaging::{Event, Job};
use app_error::{AppError, AppResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
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
    outgoing: Vec<Outgoing>,
}

/// The database transaction behind a [`Tx`]; implemented only by `app-db`.
#[async_trait]
pub trait TxBackend: Any + Send {
    /// Writes `outgoing` to the outbox and job queue, then commits.
    async fn commit(self: Box<Self>, outgoing: Vec<Outgoing>) -> AppResult<()>;
}

/// An event or job a unit of work produced, serialized, waiting for its commit.
#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    Event {
        event_type: &'static str,
        payload: Value,
    },
    Job {
        kind: &'static str,
        payload: Value,
        /// `None` runs it as soon as a worker is free.
        run_at: Option<DateTime<Utc>>,
        max_attempts: i32,
    },
}

impl Tx {
    pub fn new(backend: impl TxBackend) -> Self {
        Self {
            backend: Box::new(backend),
            outgoing: Vec::new(),
        }
    }

    /// Delivered to every subscriber, at least once, if and only if this commits.
    pub fn publish<E: Event>(&mut self, event: &E) -> AppResult<()> {
        self.outgoing.push(Outgoing::Event {
            event_type: E::TYPE,
            payload: serde_json::to_value(event).map_err(AppError::internal)?,
        });
        Ok(())
    }

    /// Runs `job` once this commits, as soon as a worker is free.
    pub fn enqueue<J: Job>(&mut self, job: &J) -> AppResult<()> {
        self.push_job(job, None)
    }

    /// Runs `job` once this commits, no earlier than `run_at` by the worker's clock.
    pub fn schedule<J: Job>(&mut self, job: &J, run_at: DateTime<Utc>) -> AppResult<()> {
        self.push_job(job, Some(run_at))
    }

    pub async fn commit(self) -> AppResult<()> {
        self.backend.commit(self.outgoing).await
    }

    /// The concrete transaction, for the db layer (`app_db::conn`).
    pub fn backend<B: TxBackend>(&mut self) -> Option<&mut B> {
        let backend: &mut dyn Any = &mut *self.backend;
        backend.downcast_mut()
    }

    fn push_job<J: Job>(&mut self, job: &J, run_at: Option<DateTime<Utc>>) -> AppResult<()> {
        self.outgoing.push(Outgoing::Job {
            kind: J::KIND,
            payload: serde_json::to_value(job).map_err(AppError::internal)?,
            run_at,
            max_attempts: J::MAX_ATTEMPTS,
        });
        Ok(())
    }
}
