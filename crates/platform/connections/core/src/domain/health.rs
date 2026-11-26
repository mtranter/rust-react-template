use chrono::{DateTime, Utc};

/// What the last **Test connection** found (ADR 0050). It is kept for the Connection's version it
/// tested, so any later change to the Connection makes it untested again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Health {
    pub outcome: Outcome,
    pub tested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    /// Why, in plain language, and never holding a secret.
    Failing(String),
}

/// What an admin sees of a Connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// It lacks its URL, or a field or secret its auth style takes.
    NotConfigured,
    Untested,
    Ok,
    Failing(String),
}

impl Status {
    pub fn of(configured: bool, health: Option<&Health>) -> Self {
        match (configured, health) {
            (false, _) => Status::NotConfigured,
            (true, None) => Status::Untested,
            (true, Some(health)) => match &health.outcome {
                Outcome::Ok => Status::Ok,
                Outcome::Failing(error) => Status::Failing(error.clone()),
            },
        }
    }
}
