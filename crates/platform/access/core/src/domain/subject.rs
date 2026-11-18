use chrono::{DateTime, TimeDelta, Utc};

/// How stale a subject's last seen may be: a sighting sooner after the one recorded, saying
/// nothing new, isn't recorded.
pub const SEEN_WITHIN: TimeDelta = TimeDelta::hours(1);

/// A subject authenticating, with what its token says of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sighting {
    pub subject: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub at: DateTime<Utc>,
}

impl Sighting {
    /// Whether recording it would change what `recorded`, the last sighting recorded, left:
    /// details a token carried that differ, or a last seen an hour or more older.
    pub fn is_news_after(&self, recorded: &Sighting) -> bool {
        let differs = |now: &Option<String>, then: &Option<String>| now.is_some() && now != then;
        differs(&self.name, &recorded.name)
            || differs(&self.email, &recorded.email)
            || self.at - recorded.at >= SEEN_WITHIN
    }
}

/// Someone admins can assign roles to: a subject seen signing in, or one assigned roles or
/// named in `APP_BOOTSTRAP_ADMINS` before it ever has, which has no details and was never seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub subject: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(minutes: i64, name: Option<&str>) -> Sighting {
        Sighting {
            subject: "ada".into(),
            name: name.map(str::to_owned),
            email: None,
            at: DateTime::<Utc>::UNIX_EPOCH + TimeDelta::minutes(minutes),
        }
    }

    #[test]
    fn a_sighting_within_the_hour_saying_nothing_new_is_no_news() {
        assert!(!seen(59, Some("Ada")).is_news_after(&seen(0, Some("Ada"))));
        assert!(!seen(30, None).is_news_after(&seen(0, Some("Ada"))));
    }

    #[test]
    fn a_sighting_an_hour_later_or_with_other_details_is_news() {
        assert!(seen(60, Some("Ada")).is_news_after(&seen(0, Some("Ada"))));
        assert!(seen(1, Some("Ada L")).is_news_after(&seen(0, Some("Ada"))));
        assert!(seen(1, Some("Ada")).is_news_after(&seen(0, None)));
    }
}
