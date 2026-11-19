//! Schedules: a module's recurring work, declared in code beside its
//! subscriptions. `migrate` registers each in `messaging.schedule`, and the
//! worker turns each slot that falls due into a job, run as a deferred job is.

use crate::Tx;
use app_error::AppResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Recurring work a module declares, run by its handler ([`RunScheduled`]).
pub trait Schedule: Send + 'static {
    /// `<module>.<what it does>`, stable: its row in `messaging.schedule` and
    /// the queue its runs wait in.
    const NAME: &'static str;
    /// Five fields, UTC: `minute hour day-of-month month day-of-week`
    /// (`0 3 * * *` is 03:00 every day). `migrate` and the worker refuse to
    /// start on one that doesn't parse. Changing it reschedules the next run.
    const CRON: &'static str;
}

/// Runs a [`Schedule`] for one slot, in the transaction that marks the run
/// done, acting as `Principal::System`. At least once, so it must be safe to
/// run again; an error rolls its writes back for a retry. After downtime it
/// runs once, for the last slot missed, so it takes `slot` (never the clock)
/// as the time it acts for.
#[async_trait]
pub trait RunScheduled<S: Schedule>: Send + Sync {
    async fn run(&self, tx: &mut Tx, slot: DateTime<Utc>) -> AppResult<()>;
}

/// A schedule as `migrate` registers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclaredSchedule {
    pub name: &'static str,
    pub cron: &'static str,
}

/// A scheduled run's payload in `messaging.job`: only gains fields.
#[derive(Debug, Serialize, Deserialize)]
pub struct Slot {
    pub slot: DateTime<Utc>,
}
