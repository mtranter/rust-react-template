//! Five-field cron expressions, always UTC (ADR 0043). Hand-rolled rather than
//! a crate's, so the workspace's time crate (ticket 0030 plans to move off
//! `chrono`) changes here alone: nothing outside this file sees how a slot is
//! found.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use std::str::FromStr;

/// `minute hour day-of-month month day-of-week`, each `*`, a number, a range
/// `a-b`, a step `*/n` or `a-b/n`, or a comma-separated list of those. Day of
/// week is `0`–`7`, both `0` and `7` Sunday. As in Vixie cron, when both day
/// fields are restricted a day matching either is due. No names (`MON`), no
/// macros (`@daily`), no seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    minutes: u64,
    hours: u64,
    days: u64,
    months: u64,
    weekdays: u64,
    days_restricted: bool,
    weekdays_restricted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{expression}` isn't a five-field cron expression: {reason}")]
pub struct CronError {
    expression: String,
    reason: String,
}

/// How far ahead a slot is looked for: past 29 February's longest gap (eight
/// years, around 2100), so an expression that ever falls due is found.
const HORIZON_YEARS: i32 = 9;

impl Cron {
    /// The first slot strictly after `instant`: a whole minute, by UTC.
    pub fn next_after(&self, instant: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let minute = instant.with_second(0)?.with_nanosecond(0)?;
        let mut at = minute + Duration::minutes(1);
        let horizon = at.year() + HORIZON_YEARS;
        while at.year() <= horizon {
            if !has(self.months, at.month()) {
                at = start_of_next_month(at)?;
            } else if !self.day_matches(at) {
                at = start_of_day(at.date_naive().succ_opt()?);
            } else if !has(self.hours, at.hour()) {
                at = at.with_minute(0)? + Duration::hours(1);
            } else if !has(self.minutes, at.minute()) {
                at += Duration::minutes(1);
            } else {
                return Some(at);
            }
        }
        None
    }

    /// The last slot at or before `now`, starting from a slot `from` that is:
    /// what a schedule runs for when it has missed several.
    pub fn last_until(&self, from: DateTime<Utc>, now: DateTime<Utc>) -> DateTime<Utc> {
        let mut last = from;
        while let Some(next) = self.next_after(last).filter(|next| *next <= now) {
            last = next;
        }
        last
    }

    fn day_matches(&self, at: DateTime<Utc>) -> bool {
        let day = has(self.days, at.day());
        let weekday = has(self.weekdays, at.weekday().num_days_from_sunday());
        match (self.days_restricted, self.weekdays_restricted) {
            (true, true) => day || weekday,
            (true, false) => day,
            (false, true) => weekday,
            (false, false) => true,
        }
    }
}

impl FromStr for Cron {
    type Err = CronError;

    fn from_str(expression: &str) -> Result<Self, CronError> {
        let refuse = |reason: String| CronError {
            expression: expression.into(),
            reason,
        };
        let fields: Vec<&str> = expression.split_whitespace().collect();
        let [minute, hour, day, month, weekday] = fields[..] else {
            return Err(refuse(format!("it has {} fields", fields.len())));
        };
        let field = |text, name, min, max| {
            parse_field(text, min, max).map_err(|e| refuse(format!("{name}: {e}")))
        };
        let mut weekdays = field(weekday, "day of week", 0, 7)?;
        if has(weekdays, 7) {
            weekdays |= 1;
        }
        let cron = Cron {
            minutes: field(minute, "minute", 0, 59)?,
            hours: field(hour, "hour", 0, 23)?,
            days: field(day, "day of month", 1, 31)?,
            months: field(month, "month", 1, 12)?,
            weekdays,
            days_restricted: !day.starts_with('*'),
            weekdays_restricted: !weekday.starts_with('*'),
        };
        let epoch = DateTime::<Utc>::UNIX_EPOCH;
        if cron.next_after(epoch).is_none() {
            return Err(refuse("it never falls due".into()));
        }
        Ok(cron)
    }
}

/// The values a field allows, as bits.
fn parse_field(text: &str, min: u32, max: u32) -> Result<u64, String> {
    let mut bits = 0;
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => (range, number(step)?),
            None => (part, 1),
        };
        if step == 0 {
            return Err(format!("`{part}` steps by 0"));
        }
        let (from, to) = match range {
            "*" => (min, max),
            _ => match range.split_once('-') {
                Some((from, to)) => (number(from)?, number(to)?),
                None if part.contains('/') => (number(range)?, max),
                None => (number(range)?, number(range)?),
            },
        };
        if from < min || to > max || from > to {
            return Err(format!("`{part}` is outside {min}-{max}"));
        }
        for value in (from..=to).step_by(step as usize) {
            bits |= 1 << value;
        }
    }
    Ok(bits)
}

fn number(text: &str) -> Result<u32, String> {
    text.parse().map_err(|_| format!("`{text}` isn't a number"))
}

fn has(bits: u64, value: u32) -> bool {
    bits & (1 << value) != 0
}

fn start_of_day(date: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_time(NaiveTime::MIN))
}

fn start_of_next_month(at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let (year, month) = match at.month() {
        12 => (at.year() + 1, 1),
        month => (at.year(), month + 1),
    };
    Some(start_of_day(NaiveDate::from_ymd_opt(year, month, 1)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cron(expression: &str) -> Cron {
        expression.parse().expect("a valid expression")
    }

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().expect("an RFC 3339 instant")
    }

    fn next(expression: &str, after: &str) -> DateTime<Utc> {
        cron(expression).next_after(at(after)).expect("a next slot")
    }

    #[test]
    fn the_next_slot_is_the_first_whole_minute_strictly_after() {
        assert_eq!(
            next("* * * * *", "2026-01-01T10:00:00Z"),
            at("2026-01-01T10:01:00Z")
        );
        assert_eq!(
            next("* * * * *", "2026-01-01T10:00:30Z"),
            at("2026-01-01T10:01:00Z")
        );
    }

    #[test]
    fn a_nightly_schedule_falls_due_once_a_day() {
        assert_eq!(
            next("0 3 * * *", "2026-01-01T02:59:00Z"),
            at("2026-01-01T03:00:00Z")
        );
        assert_eq!(
            next("0 3 * * *", "2026-01-01T03:00:00Z"),
            at("2026-01-02T03:00:00Z")
        );
        assert_eq!(
            next("0 3 * * *", "2026-12-31T04:00:00Z"),
            at("2027-01-01T03:00:00Z")
        );
    }

    #[test]
    fn lists_ranges_and_steps_combine() {
        assert_eq!(
            next("*/15 9-17 * * *", "2026-01-01T09:20:00Z"),
            at("2026-01-01T09:30:00Z")
        );
        assert_eq!(
            next("*/15 9-17 * * *", "2026-01-01T17:45:00Z"),
            at("2026-01-02T09:00:00Z")
        );
        assert_eq!(
            next("5,10 0 1 1,7 *", "2026-01-01T00:05:00Z"),
            at("2026-01-01T00:10:00Z")
        );
        assert_eq!(
            next("5,10 0 1 1,7 *", "2026-01-01T00:10:00Z"),
            at("2026-07-01T00:05:00Z")
        );
        assert_eq!(
            next("10/20 * * * *", "2026-01-01T00:31:00Z"),
            at("2026-01-01T00:50:00Z")
        );
    }

    #[test]
    fn day_of_week_counts_sunday_as_0_and_7() {
        // 2026-01-01 is a Thursday.
        assert_eq!(
            next("0 0 * * 0", "2026-01-01T00:00:00Z"),
            at("2026-01-04T00:00:00Z")
        );
        assert_eq!(
            next("0 0 * * 7", "2026-01-01T00:00:00Z"),
            at("2026-01-04T00:00:00Z")
        );
        assert_eq!(
            next("0 0 * * 1-5", "2026-01-02T00:00:00Z"),
            at("2026-01-05T00:00:00Z")
        );
    }

    #[test]
    fn both_day_fields_restricted_match_either() {
        // The 10th, or any Monday: Monday the 5th comes first.
        assert_eq!(
            next("0 0 10 * 1", "2026-01-01T00:00:00Z"),
            at("2026-01-05T00:00:00Z")
        );
        // Only the day of month restricted: the 10th.
        assert_eq!(
            next("0 0 10 * *", "2026-01-01T00:00:00Z"),
            at("2026-01-10T00:00:00Z")
        );
    }

    #[test]
    fn a_day_some_months_lack_waits_for_one_that_has_it() {
        assert_eq!(
            next("0 0 31 * *", "2026-02-01T00:00:00Z"),
            at("2026-03-31T00:00:00Z")
        );
        assert_eq!(
            next("0 0 29 2 *", "2026-01-01T00:00:00Z"),
            at("2028-02-29T00:00:00Z")
        );
    }

    #[test]
    fn the_last_slot_until_now_skips_every_missed_one_before_it() {
        let nightly = cron("0 3 * * *");
        assert_eq!(
            nightly.last_until(at("2026-01-01T03:00:00Z"), at("2026-01-04T12:00:00Z")),
            at("2026-01-04T03:00:00Z")
        );
        assert_eq!(
            nightly.last_until(at("2026-01-01T03:00:00Z"), at("2026-01-01T03:00:00Z")),
            at("2026-01-01T03:00:00Z")
        );
    }

    #[test]
    fn malformed_expressions_are_refused() {
        for expression in [
            "",
            "* * * *",
            "* * * * * *",
            "60 * * * *",
            "* 24 * * *",
            "* * 0 * *",
            "* * * 13 *",
            "* * * * 8",
            "5-1 * * * *",
            "*/0 * * * *",
            "a * * * *",
            "MON * * * *",
            "@daily",
        ] {
            assert!(
                expression.parse::<Cron>().is_err(),
                "`{expression}` was accepted"
            );
        }
    }

    #[test]
    fn an_expression_that_never_falls_due_is_refused() {
        let refused = "0 0 30 2 *".parse::<Cron>().unwrap_err();
        assert_eq!(
            refused.to_string(),
            "`0 0 30 2 *` isn't a five-field cron expression: it never falls due"
        );
    }
}
