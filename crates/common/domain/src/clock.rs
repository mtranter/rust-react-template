use chrono::{DateTime, SubsecRound, TimeDelta, Utc};
use std::sync::Mutex;

/// Where `application/` reads the time; the domain takes `now` as an argument.
/// Injected rather than ambient (spec §4), so a test can stop and move time and
/// work spawned off a request reads the same clock.
pub trait Clock: Send + Sync {
    /// At Postgres' `timestamptz` precision (microseconds), so an aggregate
    /// returned straight after `save` matches the same one read back.
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now().trunc_subsecs(6)
    }
}

/// A clock that moves only when told to: for tests.
pub struct FixedClock(Mutex<DateTime<Utc>>);

impl FixedClock {
    pub fn new(at: DateTime<Utc>) -> Self {
        Self(Mutex::new(at.trunc_subsecs(6)))
    }

    pub fn advance(&self, by: TimeDelta) {
        *self.0.lock().expect("clock lock") += by;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("clock lock")
    }
}

/// `2025-01-01T00:00:00Z` plus `minutes`: a fixed timestamp for seed data. Spacing
/// seeds apart makes "most-recent first" ordering deterministic.
pub fn seed_time(minutes: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_735_689_600 + minutes * 60, 0).expect("valid seed timestamp")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_clock_moves_only_when_advanced() {
        let clock = FixedClock::new(seed_time(0));
        assert_eq!(clock.now(), seed_time(0));
        clock.advance(TimeDelta::minutes(5));
        assert_eq!(clock.now(), seed_time(5));
    }
}
