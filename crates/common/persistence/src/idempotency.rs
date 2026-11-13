//! Idempotency records: what a request sent with an `Idempotency-Key` was
//! answered with, so a retry gets the same answer instead of running again.
//! Pure: `app-db` stores them and `app-web` decides when.

use app_error::AppResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

#[async_trait]
pub trait IdempotencyRecords: Send + Sync {
    /// Claims `claim.key` for `request` at `claim.at`, unless a record made after
    /// `expired_before` holds it already, or the same request claimed it after
    /// `abandoned_before` and hasn't been answered.
    async fn claim(
        &self,
        claim: &KeyClaim,
        request: &[u8],
        expired_before: DateTime<Utc>,
        abandoned_before: DateTime<Utc>,
    ) -> AppResult<Claimed>;

    /// Keeps the answer to a request `claim` holds the key for.
    async fn complete(&self, claim: &KeyClaim, response: &RecordedResponse) -> AppResult<()>;

    /// Gives the key up, so a retry runs the request again.
    async fn release(&self, claim: &KeyClaim) -> AppResult<()>;
}

/// A caller's key, claimed at `at`; a claim made later, after this one
/// expired, isn't this one's to complete or release.
#[derive(Debug, Clone)]
pub struct KeyClaim {
    /// Whose key it is: the bearer token's subject, or empty for a caller
    /// who sent none, so one caller's key never answers another's request.
    pub caller: String,
    pub key: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claimed {
    /// The key is this request's: run it.
    Claimed,
    /// The key was claimed for a different request.
    ForAnotherRequest,
    /// The same request holds the key and hasn't been answered yet.
    InFlight,
    /// The same request was answered with this.
    Answered(RecordedResponse),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
