use chrono::{DateTime, SubsecRound, Utc};

/// The current time at Postgres' `timestamptz` precision (microseconds), so an
/// aggregate returned straight after `save` matches the same one read back.
/// Only `application/` calls this; the domain takes `now` as an argument.
pub fn now() -> DateTime<Utc> {
    Utc::now().trunc_subsecs(6)
}

/// `2025-01-01T00:00:00Z` plus `minutes`: a fixed timestamp for seed data. Spacing
/// seeds apart makes "most-recent first" ordering deterministic.
pub fn seed_time(minutes: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_735_689_600 + minutes * 60, 0).expect("valid seed timestamp")
}
