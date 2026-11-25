use super::{Assignment, GrantedBy, InvitationError, InvitationEvent, InvitationId};
use chrono::{DateTime, Utc};
use std::fmt;

const MAX_EMAIL_LEN: usize = 254;
const MAX_NAME_LEN: usize = 200;

/// An address an invitation can be sent to: trimmed and lower-cased, so one person has one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Email(String);

impl Email {
    pub fn parse(raw: &str) -> Result<Self, InvitationError> {
        let email = raw.trim().to_ascii_lowercase();
        if email.chars().count() > MAX_EMAIL_LEN {
            return Err(InvitationError::InvalidEmail);
        }
        // Deliberately minimal: the identity provider has the last word.
        match email.split_once('@') {
            Some((local, domain))
                if !local.is_empty()
                    && domain.contains('.')
                    && !domain.starts_with('.')
                    && !domain.ends_with('.')
                    && !email.chars().any(char::is_whitespace) =>
            {
                Ok(Self(email))
            }
            _ => Err(InvitationError::InvalidEmail),
        }
    }

    /// Skips validation: for repositories only, as stored values were valid when saved.
    pub fn restore(email: String) -> Self {
        Self(email)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone)]
pub struct InviteUser {
    pub email: String,
    pub name: String,
    pub roles: Vec<String>,
}

/// An invitation's inputs, checked before the identity provider is asked for anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub email: Email,
    pub name: String,
    /// Trimmed, without repeats, in the order given.
    pub roles: Vec<String>,
}

impl Invite {
    pub fn parse(cmd: InviteUser) -> Result<Self, InvitationError> {
        let email = Email::parse(&cmd.email)?;
        let name = cmd.name.trim();
        if name.chars().count() > MAX_NAME_LEN {
            return Err(InvitationError::NameTooLong { max: MAX_NAME_LEN });
        }
        let mut roles: Vec<String> = Vec::new();
        for role in &cmd.roles {
            let role = Assignment::role_of(role).map_err(InvitationError::Role)?;
            if !roles.iter().any(|r| r == role) {
                roles.push(role.to_owned());
            }
        }
        Ok(Self {
            email,
            name: name.to_owned(),
            roles,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum InvitationStatus {
    /// The invitee hasn't signed in yet.
    Pending,
    Accepted,
    Revoked,
}

/// Someone an admin asked the identity provider to create, holding the roles the admin chose
/// from the moment the provider answers with their `sub`, until it is revoked before they sign in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    id: InvitationId,
    email: Email,
    name: String,
    subject: String,
    roles: Vec<String>,
    invited_by: GrantedBy,
    invited_at: DateTime<Utc>,
    status: InvitationStatus,
}

impl Invitation {
    /// `subject` is the `sub` the identity provider gave the invitee.
    pub fn invite(
        id: InvitationId,
        invite: Invite,
        subject: String,
        by: GrantedBy,
        now: DateTime<Utc>,
    ) -> (Self, InvitationEvent) {
        let invitation = Self {
            id,
            email: invite.email,
            name: invite.name,
            subject,
            roles: invite.roles,
            invited_by: by,
            invited_at: now,
            status: InvitationStatus::Pending,
        };
        (invitation, InvitationEvent::Created)
    }

    pub fn resend(&self, by: GrantedBy) -> Result<InvitationEvent, InvitationError> {
        self.ensure_pending()?;
        Ok(InvitationEvent::Resent { by })
    }

    /// Nothing happens to one no longer pending: the invitee signs in many times.
    pub fn accept(&mut self) -> Option<InvitationEvent> {
        if self.status != InvitationStatus::Pending {
            return None;
        }
        self.status = InvitationStatus::Accepted;
        Some(InvitationEvent::Accepted)
    }

    pub fn revoke(&mut self, by: GrantedBy) -> Result<InvitationEvent, InvitationError> {
        self.ensure_pending()?;
        self.status = InvitationStatus::Revoked;
        Ok(InvitationEvent::Revoked { by })
    }

    fn ensure_pending(&self) -> Result<(), InvitationError> {
        match self.status {
            InvitationStatus::Pending => Ok(()),
            InvitationStatus::Accepted => Err(InvitationError::Accepted),
            InvitationStatus::Revoked => Err(InvitationError::Revoked),
        }
    }

    /// Skips invariants: for repositories rebuilding persisted state only.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: InvitationId,
        email: Email,
        name: String,
        subject: String,
        roles: Vec<String>,
        invited_by: GrantedBy,
        invited_at: DateTime<Utc>,
        status: InvitationStatus,
    ) -> Self {
        Self {
            id,
            email,
            name,
            subject,
            roles,
            invited_by,
            invited_at,
            status,
        }
    }

    pub fn id(&self) -> InvitationId {
        self.id
    }

    pub fn email(&self) -> &Email {
        &self.email
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn roles(&self) -> &[String] {
        &self.roles
    }

    pub fn invited_by(&self) -> &GrantedBy {
        &self.invited_by
    }

    pub fn invited_at(&self) -> DateTime<Utc> {
        self.invited_at
    }

    pub fn status(&self) -> InvitationStatus {
        self.status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::AssignmentError;

    fn parsing(email: &str, roles: &[&str]) -> Result<Invite, InvitationError> {
        Invite::parse(InviteUser {
            email: email.into(),
            name: " Ada ".into(),
            roles: roles.iter().map(|r| (*r).to_owned()).collect(),
        })
    }

    fn pending() -> Invitation {
        Invitation::invite(
            InvitationId::new(),
            parsing("ada@example.com", &["support"]).unwrap(),
            "sub-ada".into(),
            GrantedBy::System,
            Utc::now(),
        )
        .0
    }

    #[test]
    fn an_email_is_trimmed_lower_cased_and_must_look_like_one() {
        let invite = parsing(" Ada@Example.COM ", &[]).unwrap();
        assert_eq!(invite.email.as_str(), "ada@example.com");
        assert_eq!(invite.name, "Ada");
        for bad in [
            "",
            "ada",
            "@example.com",
            "ada@example",
            "ada@.com",
            "a da@example.com",
        ] {
            assert_eq!(
                parsing(bad, &[]),
                Err(InvitationError::InvalidEmail),
                "{bad}"
            );
        }
    }

    #[test]
    fn roles_are_trimmed_without_repeats_and_never_user() {
        let invite = parsing("ada@example.com", &[" support ", "support", "ops"]).unwrap();
        assert_eq!(invite.roles, ["support", "ops"]);
        assert_eq!(
            parsing("ada@example.com", &["user"]),
            Err(InvitationError::Role(AssignmentError::Implicit))
        );
    }

    #[test]
    fn only_a_pending_invitation_is_resent_accepted_or_revoked() {
        let mut accepted = pending();
        assert_eq!(accepted.accept(), Some(InvitationEvent::Accepted));
        assert_eq!(accepted.accept(), None);
        assert_eq!(
            accepted.resend(GrantedBy::System),
            Err(InvitationError::Accepted)
        );
        assert_eq!(
            accepted.revoke(GrantedBy::System),
            Err(InvitationError::Accepted)
        );

        let mut revoked = pending();
        assert!(revoked.revoke(GrantedBy::System).is_ok());
        assert_eq!(revoked.status(), InvitationStatus::Revoked);
        assert_eq!(revoked.accept(), None);
        assert_eq!(
            revoked.revoke(GrantedBy::System),
            Err(InvitationError::Revoked)
        );
    }
}
