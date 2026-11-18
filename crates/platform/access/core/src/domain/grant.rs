use super::{GrantError, GrantEvent, GrantId, Scope};
use chrono::{DateTime, Utc};

const MAX_ROLE_LEN: usize = 100;

/// Who made a grant, or revoked one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantedBy {
    /// `migrate`, applying a module's default grant.
    System,
    User(String),
}

impl GrantedBy {
    pub fn subject(&self) -> Option<&str> {
        match self {
            GrantedBy::System => None,
            GrantedBy::User(id) => Some(id),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GrantAccess {
    pub role: String,
    pub resource: String,
    pub action: String,
    pub scope: Scope,
}

/// Role → action, within a scope (ADR 0040). Whether its resource, action and scope exist is
/// for the loaded policies to say, which the service asks before saving it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    id: GrantId,
    rule: app_authz::Grant,
    granted_by: GrantedBy,
    granted_at: DateTime<Utc>,
}

impl Grant {
    pub fn make(
        id: GrantId,
        cmd: GrantAccess,
        by: GrantedBy,
        now: DateTime<Utc>,
    ) -> Result<(Self, GrantEvent), GrantError> {
        let role = cmd.role.trim();
        if role.is_empty() {
            return Err(GrantError::EmptyRole);
        }
        if role.chars().count() > MAX_ROLE_LEN {
            return Err(GrantError::RoleTooLong { max: MAX_ROLE_LEN });
        }
        let grant = Self {
            id,
            rule: app_authz::Grant {
                role: role.to_owned(),
                resource: cmd.resource,
                action: cmd.action,
                scope: cmd.scope,
            },
            granted_by: by,
            granted_at: now,
        };
        Ok((grant, GrantEvent::Granted))
    }

    pub fn revoke(&self, by: GrantedBy) -> GrantEvent {
        GrantEvent::Revoked { by }
    }

    /// Skips invariants: for repositories rebuilding persisted state only.
    pub fn restore(
        id: GrantId,
        rule: app_authz::Grant,
        granted_by: GrantedBy,
        granted_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            rule,
            granted_by,
            granted_at,
        }
    }

    pub fn id(&self) -> GrantId {
        self.id
    }

    /// What the rule set compiles it to.
    pub fn rule(&self) -> &app_authz::Grant {
        &self.rule
    }

    pub fn role(&self) -> &str {
        &self.rule.role
    }

    pub fn resource(&self) -> &str {
        &self.rule.resource
    }

    pub fn action(&self) -> &str {
        &self.rule.action
    }

    pub fn scope(&self) -> &Scope {
        &self.rule.scope
    }

    pub fn granted_by(&self) -> &GrantedBy {
        &self.granted_by
    }

    pub fn granted_at(&self) -> DateTime<Utc> {
        self.granted_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn granting(role: &str) -> Result<Grant, GrantError> {
        Grant::make(
            GrantId::new(),
            GrantAccess {
                role: role.into(),
                resource: "gadget".into(),
                action: "make".into(),
                scope: Scope::Any,
            },
            GrantedBy::System,
            Utc::now(),
        )
        .map(|(grant, _)| grant)
    }

    #[test]
    fn a_role_is_trimmed_and_bounded() {
        assert_eq!(granting(" support ").unwrap().role(), "support");
        assert_eq!(granting("  "), Err(GrantError::EmptyRole));
        assert_eq!(
            granting(&"r".repeat(MAX_ROLE_LEN + 1)),
            Err(GrantError::RoleTooLong { max: MAX_ROLE_LEN })
        );
    }
}
