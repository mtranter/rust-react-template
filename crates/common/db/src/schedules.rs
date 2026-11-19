//! Schedules (ADR 0043): each module's recurring work, kept in
//! `messaging.schedule` rather than in each process's memory, so workers of
//! every build running during a rolling deploy create each slot once.

use crate::cron::{Cron, CronError};
use app_domain::Clock;
use app_persistence::{DeclaredSchedule, Handlers};
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};

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

/// Registers every schedule `handlers` declare, and retires every one they
/// don't: `migrate` runs it after the migrations, and before writing anything
/// refuses a build with an expression that doesn't parse. A new schedule is
/// first due at its next slot after now; one whose expression changed, or
/// that was retired, is (re)scheduled to its next slot; one unchanged keeps
/// its due time. A retired schedule makes no more runs: its pending run is
/// deleted, and one a worker has claimed is dead-lettered, so it finishes
/// (its success removes it) or stays dead, and never runs again.
pub async fn register_schedules(
    pool: &PgPool,
    clock: &dyn Clock,
    handlers: &Handlers,
) -> Result<(), ScheduleError> {
    let schedules = parse_schedules(handlers)?;
    let now = clock.now();
    let mut tx = pool.begin().await?;
    // EXCLUSIVE, before any row: it serialises two `migrate`s, waits for a
    // worker's `schedule` pass in flight to commit the runs it creates, and
    // holds off the next until this commits. SHARE ROW EXCLUSIVE, as
    // subscriptions take, would deadlock with a pass: granted beside the
    // pass's row locks, it blocks the pass's update of `next_at` while this
    // waits for those rows.
    sqlx::query!("LOCK TABLE messaging.schedule IN EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    for (declared, cron) in &schedules {
        // `parse` refuses an expression with no next slot.
        let next_at = cron.next_after(now).expect("a parsed expression falls due");
        sqlx::query!(
            r#"
            INSERT INTO messaging.schedule (name, cron, next_at, registered_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (name) DO UPDATE
            SET cron = EXCLUDED.cron, next_at = EXCLUDED.next_at, retired_at = NULL
            WHERE messaging.schedule.cron <> EXCLUDED.cron
               OR messaging.schedule.retired_at IS NOT NULL
            "#,
            declared.name,
            declared.cron,
            next_at,
            now,
        )
        .execute(&mut *tx)
        .await?;
    }
    let declared: Vec<&str> = schedules.iter().map(|(s, _)| s.name).collect();
    retire_undeclared(&mut tx, now, &declared).await?;
    tx.commit().await?;
    Ok(())
}

async fn retire_undeclared(
    conn: &mut PgConnection,
    now: DateTime<Utc>,
    declared: &[&str],
) -> sqlx::Result<()> {
    let retired = sqlx::query_scalar!(
        r#"
        UPDATE messaging.schedule SET retired_at = $1
        WHERE retired_at IS NULL AND name <> ALL($2::text[])
        RETURNING name
        "#,
        now,
        declared as &[&str],
    )
    .fetch_all(&mut *conn)
    .await?;
    if retired.is_empty() {
        return Ok(());
    }
    // A run is claimed by raising its `attempts` (`Worker::run_next`), in a
    // statement of its own: each statement below waits for a claim in flight
    // and sees it, and one that locks the row first makes the claim skip it.
    sqlx::query!(
        r#"
        DELETE FROM messaging.job
        WHERE deferred AND queue = ANY($1) AND key = queue AND dead_at IS NULL AND attempts = 0
        "#,
        &retired,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        r#"
        UPDATE messaging.job SET dead_at = $2, last_error = 'schedule retired'
        WHERE deferred AND queue = ANY($1) AND key = queue AND dead_at IS NULL
        "#,
        &retired,
        now,
    )
    .execute(&mut *conn)
    .await?;
    for name in &retired {
        tracing::info!(schedule = name, "schedule retired: no build declares it");
    }
    Ok(())
}
