//! Schedules (ADR 0043): each module's recurring work, kept in
//! `messaging.schedule` rather than in each process's memory, so workers of
//! every build running during a rolling deploy create each slot once.

use crate::cron::{Cron, CronError};
use app_domain::Clock;
use app_persistence::{DeclaredSchedule, Handlers};
use sqlx::PgPool;

/// Why `migrate` refused to register a build's schedules, or a worker to start.
#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error("the schedule `{name}` can't run: {source}")]
    InvalidCron {
        name: &'static str,
        #[source]
        source: CronError,
    },
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Each schedule `handlers` declare, with its expression parsed; refuses the
/// first that doesn't parse, naming it.
pub fn parse_schedules(
    handlers: &Handlers,
) -> Result<Vec<(DeclaredSchedule, Cron)>, ScheduleError> {
    handlers
        .schedules()
        .map(|s| {
            s.cron
                .parse()
                .map(|cron| (s, cron))
                .map_err(|source| ScheduleError::InvalidCron {
                    name: s.name,
                    source,
                })
        })
        .collect()
}

/// Registers every schedule `handlers` declare: `migrate` runs it after the
/// migrations, and before writing anything refuses a build with an expression
/// that doesn't parse. A new schedule is first due at its next slot after
/// now; one whose expression changed is rescheduled to the new expression's
/// next slot; one unchanged keeps its due time.
pub async fn register_schedules(
    pool: &PgPool,
    clock: &dyn Clock,
    handlers: &Handlers,
) -> Result<(), ScheduleError> {
    let schedules = parse_schedules(handlers)?;
    let now = clock.now();
    let mut tx = pool.begin().await?;
    for (declared, cron) in &schedules {
        // `parse` refuses an expression with no next slot.
        let next_at = cron.next_after(now).expect("a parsed expression falls due");
        sqlx::query!(
            r#"
            INSERT INTO messaging.schedule (name, cron, next_at, registered_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (name) DO UPDATE SET cron = EXCLUDED.cron, next_at = EXCLUDED.next_at
            WHERE messaging.schedule.cron <> EXCLUDED.cron
            "#,
            declared.name,
            declared.cron,
            next_at,
            now,
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
