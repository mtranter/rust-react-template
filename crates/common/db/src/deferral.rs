use crate::conn;
use app_domain::Clock;
use app_error::{AppError, AppResult};
use app_persistence::{DeferredJobs, MAX_ATTEMPTS, PendingJob, Tx};
use async_trait::async_trait;
use std::sync::Arc;

/// Writes deferred jobs into `messaging.job`, in the caller's transaction,
/// for the worker to run once due. The clock stamps when each was deferred.
pub struct PgDeferredJobs {
    clock: Arc<dyn Clock>,
}

impl PgDeferredJobs {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self { clock }
    }
}

#[async_trait]
impl DeferredJobs for PgDeferredJobs {
    /// A replacement starts its attempts afresh and takes the next `version`,
    /// so a run of the job it replaces, still under way, completes nothing
    /// and its writes roll back.
    async fn write(&self, tx: &mut Tx, job: PendingJob) -> AppResult<()> {
        let (name, key, run_at) = (job.name, job.key.clone(), job.run_at);
        let payload = job.payload()?;
        sqlx::query!(
            r#"
            INSERT INTO messaging.job
                (queue, event_type, key, version, payload, outbox_seq, run_at, max_attempts,
                 created_at, deferred)
            VALUES ($1, $1, $2, 1, $3, 0, $4, $5, $6, true)
            ON CONFLICT (queue, key) WHERE deferred AND dead_at IS NULL
            DO UPDATE SET payload = EXCLUDED.payload, run_at = EXCLUDED.run_at,
                          version = messaging.job.version + 1, attempts = 0, last_error = NULL
            "#,
            name,
            key,
            payload,
            run_at,
            MAX_ATTEMPTS,
            self.clock.now(),
        )
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }

    async fn remove(&self, tx: &mut Tx, name: &'static str, key: String) -> AppResult<()> {
        sqlx::query!(
            r#"
            DELETE FROM messaging.job
            WHERE queue = $1 AND key = $2 AND deferred AND dead_at IS NULL
            "#,
            name,
            key,
        )
        .execute(conn(tx))
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }
}
