use super::{AssignmentError, AssignmentEvent, GrantedBy};
use app_authz::USER;
use chrono::{DateTime, Utc};

const MAX_SUBJECT_LEN: usize = 255;

#[derive(Debug, Clone)]
pub struct AssignRole {
    pub subject: String,
    pub role: String,
}

/// A role assigned to a subject (the token's `sub`), identified by the two. A user holds `user`
/// and every role assigned to them; `user` itself is never assigned. Whether the role exists is
/// for the store to say, which refuses the assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    subject: String,
    role: String,
    assigned_by: GrantedBy,
    assigned_at: DateTime<Utc>,
}

impl Assignment {
    pub fn assign(
        cmd: AssignRole,
        by: GrantedBy,
        now: DateTime<Utc>,
    ) -> Result<(Self, AssignmentEvent), AssignmentError> {
        let subject = Self::subject_of(&cmd.subject)?;
        let role = Self::role_of(&cmd.role)?;
        let assignment = Self {
            subject: subject.to_owned(),
            role: role.to_owned(),
            assigned_by: by,
            assigned_at: now,
        };
        Ok((assignment, AssignmentEvent::Assigned))
    }

    /// The subject an assignment may name, trimmed.
    pub fn subject_of(subject: &str) -> Result<&str, AssignmentError> {
        let subject = subject.trim();
        if subject.is_empty() {
            return Err(AssignmentError::EmptySubject);
        }
        if subject.chars().count() > MAX_SUBJECT_LEN {
            return Err(AssignmentError::SubjectTooLong {
                max: MAX_SUBJECT_LEN,
            });
        }
        Ok(subject)
    }

    /// The role an assignment may name, trimmed: never `user`, which every subject holds.
    pub fn role_of(role: &str) -> Result<&str, AssignmentError> {
        let role = role.trim();
        if role.is_empty() {
            return Err(AssignmentError::EmptyRole);
        }
        if role == USER {
            return Err(AssignmentError::Implicit);
        }
        Ok(role)
    }

    /// Whether it is a bootstrap admin's `admin`, or the last `admin`, is for the service to say.
    pub fn unassign(&self, by: GrantedBy) -> AssignmentEvent {
        AssignmentEvent::Unassigned { by }
    }

    /// Skips invariants: for repositories rebuilding persisted state only.
    pub fn restore(
        subject: String,
        role: String,
        assigned_by: GrantedBy,
        assigned_at: DateTime<Utc>,
    ) -> Self {
        Self {
            subject,
            role,
            assigned_by,
            assigned_at,
        }
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn role(&self) -> &str {
        &self.role
    }

    pub fn assigned_by(&self) -> &GrantedBy {
        &self.assigned_by
    }

    pub fn assigned_at(&self) -> DateTime<Utc> {
        self.assigned_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assigning(subject: &str, role: &str) -> Result<Assignment, AssignmentError> {
        Assignment::assign(
            AssignRole {
                subject: subject.into(),
                role: role.into(),
            },
            GrantedBy::System,
            Utc::now(),
        )
        .map(|(assignment, _)| assignment)
    }

    #[test]
    fn a_subject_and_role_are_trimmed_and_required() {
        let assignment = assigning(" ada ", " support ").unwrap();
        assert_eq!(
            (assignment.subject(), assignment.role()),
            ("ada", "support")
        );
        assert_eq!(
            assigning(" ", "support"),
            Err(AssignmentError::EmptySubject)
        );
        assert_eq!(assigning("ada", " "), Err(AssignmentError::EmptyRole));
        assert_eq!(
            assigning(&"s".repeat(MAX_SUBJECT_LEN + 1), "support"),
            Err(AssignmentError::SubjectTooLong {
                max: MAX_SUBJECT_LEN
            })
        );
    }

    #[test]
    fn user_is_never_assigned() {
        assert_eq!(assigning("ada", "user"), Err(AssignmentError::Implicit));
        assert_eq!(
            Assignment::role_of(" user "),
            Err(AssignmentError::Implicit)
        );
    }
}
