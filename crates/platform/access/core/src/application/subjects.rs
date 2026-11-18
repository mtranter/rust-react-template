use super::{AccessService, ListQuery, Listed, SubjectFilter, SubjectRepository, SubjectSortKey};
use super::{Version, Versioned};
use crate::domain::{Assignment, SEEN_WITHIN, Sighting, Subject, SubjectAction};
use app_authz::{ADMIN, Principal};
use app_domain::Clock;
use app_error::AppResult;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A role a subject holds beyond `user`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldRole {
    pub role: String,
    /// The assignment's, for `unassign_role`'s `If-Match`; `None` for a bootstrap admin's
    /// `admin` that isn't also assigned.
    pub version: Option<Version>,
}

#[derive(Debug, Clone)]
pub struct SubjectListing {
    pub subject: Subject,
    pub roles: Vec<HeldRole>,
    /// In `APP_BOOTSTRAP_ADMINS`, so it holds `admin` whatever is assigned.
    pub bootstrap_admin: bool,
}

impl AccessService {
    pub async fn list_subjects(
        &self,
        principal: &Principal,
        filter: SubjectFilter,
        query: ListQuery<SubjectSortKey>,
    ) -> AppResult<Listed<SubjectListing>> {
        let rows = self
            .authorizer
            .rows::<Subject>(principal, SubjectAction::List)?;
        let listed = self
            .subjects
            .list(&filter, self.authorizer.bootstrap_admins(), &query, &rows)
            .await?;
        let ids: Vec<String> = listed.items.iter().map(|s| s.subject.clone()).collect();
        let mut held: HashMap<String, Vec<HeldRole>> = HashMap::new();
        for Versioned { entity, version } in self.assignments.of_subjects(&ids).await? {
            held.entry(entity.subject().to_owned())
                .or_default()
                .push(held_role(&entity, version));
        }
        Ok(Listed {
            items: listed
                .items
                .into_iter()
                .map(|subject| {
                    let mut roles = held.remove(&subject.subject).unwrap_or_default();
                    let bootstrap_admin = self.authorizer.is_bootstrap_admin(&subject.subject);
                    if bootstrap_admin && !roles.iter().any(|r| r.role == ADMIN) {
                        roles.insert(
                            0,
                            HeldRole {
                                role: ADMIN.to_owned(),
                                version: None,
                            },
                        );
                    }
                    SubjectListing {
                        subject,
                        roles,
                        bootstrap_admin,
                    }
                })
                .collect(),
            total: listed.total,
        })
    }
}

fn held_role(assignment: &Assignment, version: Version) -> HeldRole {
    HeldRole {
        role: assignment.role().to_owned(),
        version: Some(version),
    }
}

/// Past this many subjects remembered, those last recorded over `SEEN_WITHIN` ago are forgotten.
const REMEMBERED: usize = 10_000;

/// Records each subject that authenticates. Remembers what it last recorded of each, so a
/// subject's requests within `SEEN_WITHIN` of that, saying nothing new, reach no store; another
/// instance recording it meanwhile is caught by the store's own check.
pub struct SignIns {
    subjects: Arc<dyn SubjectRepository>,
    clock: Arc<dyn Clock>,
    recorded: Mutex<HashMap<String, Sighting>>,
}

impl SignIns {
    pub fn new(subjects: Arc<dyn SubjectRepository>, clock: Arc<dyn Clock>) -> Self {
        Self {
            subjects,
            clock,
            recorded: Mutex::new(HashMap::new()),
        }
    }

    pub async fn seen(
        &self,
        subject: &str,
        name: Option<String>,
        email: Option<String>,
    ) -> AppResult<()> {
        let sighting = Sighting {
            subject: subject.to_owned(),
            name,
            email,
            at: self.clock.now(),
        };
        if let Some(last) = self.remembered(subject)
            && !sighting.is_news_after(&last)
        {
            return Ok(());
        }
        self.subjects
            .record(&sighting, sighting.at - SEEN_WITHIN)
            .await?;
        let mut recorded = self.recorded.lock().unwrap_or_else(|e| e.into_inner());
        if recorded.len() >= REMEMBERED {
            recorded.retain(|_, last| sighting.at - last.at < SEEN_WITHIN);
        }
        let merged = match recorded.remove(subject) {
            Some(last) => Sighting {
                name: sighting.name.or(last.name),
                email: sighting.email.or(last.email),
                ..sighting
            },
            None => sighting,
        };
        recorded.insert(subject.to_owned(), merged);
        Ok(())
    }

    fn remembered(&self, subject: &str) -> Option<Sighting> {
        self.recorded
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(subject)
            .cloned()
    }
}
