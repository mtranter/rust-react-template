use super::{
    AccessService, EventPublisher, InvitationRepository, ListQuery, Listed, SubjectFilter,
    SubjectRepository, SubjectSortKey, Transactions, events,
};
use super::{Version, Versioned};
use crate::domain::{Assignment, SEEN_WITHIN, Sighting, Subject, SubjectAction};
use app_authz::{ADMIN, Principal};
use app_domain::Clock;
use app_error::AppResult;
use app_persistence::Expected;
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

/// Past this many subjects remembered, those last recorded over `SEEN_WITHIN` ago are forgotten,
/// or every one if none was.
const REMEMBERED: usize = 10_000;

/// Records each subject that authenticates, and accepts the pending invitation of one seen for
/// the first time since it was invited, in the same unit of work. Remembers what it last recorded
/// of each, so a subject's requests within `SEEN_WITHIN` of that, saying nothing new, reach no
/// store; another instance recording it meanwhile is caught by the store's own check.
pub struct SignIns {
    subjects: Arc<dyn SubjectRepository>,
    invitations: Arc<dyn InvitationRepository>,
    publisher: Arc<dyn EventPublisher>,
    transactions: Arc<dyn Transactions>,
    clock: Arc<dyn Clock>,
    recorded: Mutex<HashMap<String, Sighting>>,
}

impl SignIns {
    pub fn new(
        subjects: Arc<dyn SubjectRepository>,
        invitations: Arc<dyn InvitationRepository>,
        publisher: Arc<dyn EventPublisher>,
        transactions: Arc<dyn Transactions>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            subjects,
            invitations,
            publisher,
            transactions,
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
        let mut tx = self.transactions.begin().await?;
        let wrote = self
            .subjects
            .record(&mut tx, &sighting, sighting.at - SEEN_WITHIN)
            .await?;
        if wrote
            && let Some(Versioned {
                entity: mut invitation,
                version,
            }) = self.invitations.pending_of(&mut tx, subject).await?
            && let Some(event) = invitation.accept()
        {
            let version = self
                .invitations
                .save(&mut tx, &invitation, Expected::Version(version))
                .await?;
            self.publisher
                .publish(
                    &mut tx,
                    events::invitation_pending(&invitation, version, event),
                )
                .await?;
        }
        tx.commit().await?;
        let mut recorded = self.recorded.lock().unwrap_or_else(|e| e.into_inner());
        if recorded.len() >= REMEMBERED {
            recorded.retain(|_, last| sighting.at - last.at < SEEN_WITHIN);
            // All seen within the hour: forgetting them costs each one more write, not growth.
            if recorded.len() >= REMEMBERED {
                recorded.clear();
            }
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
