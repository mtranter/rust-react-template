use app_error::{AppError, AppResult};
use app_persistence::{Claimed, IdempotencyRecords, KeyClaim, RecordedResponse};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// Keeps idempotency records in `idempotency.request`, each statement committed
/// on its own: a claim must be seen by a concurrent duplicate while the request
/// it holds the key for is still running.
pub struct PgIdempotencyRecords {
    pool: PgPool,
}

impl PgIdempotencyRecords {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IdempotencyRecords for PgIdempotencyRecords {
    async fn claim(
        &self,
        claim: &KeyClaim,
        request: &[u8],
        expired_before: DateTime<Utc>,
        abandoned_before: DateTime<Utc>,
    ) -> AppResult<Claimed> {
        // There are no scheduled jobs, so each claim clears what has expired.
        sqlx::query!(
            "DELETE FROM idempotency.request WHERE claimed_at <= $1",
            expired_before,
        )
        .execute(&self.pool)
        .await
        .map_err(AppError::internal)?;

        let claimed = sqlx::query_scalar!(
            r#"
            INSERT INTO idempotency.request AS held (caller, key, request_hash, claimed_at)
            VALUES ($1, $2, sha256($3), $4)
            ON CONFLICT (caller, key) DO UPDATE
            SET request_hash = EXCLUDED.request_hash, claimed_at = EXCLUDED.claimed_at,
                status = NULL, headers = NULL, body = NULL
            WHERE held.claimed_at <= $5
               OR (held.status IS NULL AND held.claimed_at <= $6
                   AND held.request_hash = EXCLUDED.request_hash)
            RETURNING true AS "claimed!"
            "#,
            claim.caller,
            claim.key,
            request,
            claim.at,
            expired_before,
            abandoned_before,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;
        if claimed.is_some() {
            return Ok(Claimed::Claimed);
        }

        let held = sqlx::query!(
            r#"
            SELECT request_hash = sha256($3) AS "same_request!", status, headers, body
            FROM idempotency.request
            WHERE caller = $1 AND key = $2
            "#,
            claim.caller,
            claim.key,
            request,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::internal)?;
        // Gone since the insert: released by a request that failed, so in
        // flight as far as this one can tell; a retry claims it.
        let Some(held) = held else {
            return Ok(Claimed::InFlight);
        };
        if !held.same_request {
            return Ok(Claimed::ForAnotherRequest);
        }
        let (Some(status), Some(headers), Some(body)) = (held.status, held.headers, held.body)
        else {
            return Ok(Claimed::InFlight);
        };
        Ok(Claimed::Answered(RecordedResponse {
            status: u16::try_from(status).map_err(AppError::internal)?,
            headers: serde_json::from_value(headers).map_err(AppError::internal)?,
            body,
        }))
    }

    async fn complete(&self, claim: &KeyClaim, response: &RecordedResponse) -> AppResult<()> {
        let kept = sqlx::query!(
            r#"
            UPDATE idempotency.request SET status = $4, headers = $5, body = $6
            WHERE caller = $1 AND key = $2 AND claimed_at = $3
            "#,
            claim.caller,
            claim.key,
            claim.at,
            i16::try_from(response.status).map_err(AppError::internal)?,
            serde_json::to_value(&response.headers).map_err(AppError::internal)?,
            response.body,
        )
        .execute(&self.pool)
        .await
        .map_err(AppError::internal)?;
        if kept.rows_affected() == 0 {
            tracing::warn!(
                caller = %claim.caller,
                key = %claim.key,
                "an idempotent answer was not kept: the request outlived its claim, which a retry took over"
            );
        }
        Ok(())
    }

    async fn release(&self, claim: &KeyClaim) -> AppResult<()> {
        sqlx::query!(
            r#"
            DELETE FROM idempotency.request
            WHERE caller = $1 AND key = $2 AND claimed_at = $3 AND status IS NULL
            "#,
            claim.caller,
            claim.key,
            claim.at,
        )
        .execute(&self.pool)
        .await
        .map_err(AppError::internal)?;
        Ok(())
    }
}
