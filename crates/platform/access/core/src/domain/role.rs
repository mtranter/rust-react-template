use super::{GrantedBy, RoleError, RoleEvent};
use chrono::{DateTime, Utc};

const MAX_NAME_LEN: usize = 100;
const MAX_DESCRIPTION_LEN: usize = 500;

/// The roles every app has, which can't be deleted: `admin` reaches `access` by its baseline
/// rules, and `user` is every signed-in user's.
pub const BUILT_IN_ROLES: &[&str] = &["admin", "user"];

#[derive(Debug, Clone)]
pub struct CreateRole {
    pub name: String,
    pub description: String,
}

/// What grants are made to (ADR 0040). Its name is its id, so it is never renamed; nor is it
/// otherwise changed: a role is created and deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    name: String,
    description: String,
    created_by: GrantedBy,
    created_at: DateTime<Utc>,
}

impl Role {
    pub fn create(
        cmd: CreateRole,
        by: GrantedBy,
        now: DateTime<Utc>,
    ) -> Result<(Self, RoleEvent), RoleError> {
        let name = cmd.name.trim();
        if name.is_empty() {
            return Err(RoleError::EmptyName);
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(RoleError::NameTooLong { max: MAX_NAME_LEN });
        }
        let description = cmd.description.trim();
        if description.chars().count() > MAX_DESCRIPTION_LEN {
            return Err(RoleError::DescriptionTooLong {
                max: MAX_DESCRIPTION_LEN,
            });
        }
        let role = Self {
            name: name.to_owned(),
            description: description.to_owned(),
            created_by: by,
            created_at: now,
        };
        Ok((role, RoleEvent::Created))
    }

    /// Whether a grant still names it is for the store to say, which refuses the delete.
    pub fn delete(&self, by: GrantedBy) -> Result<RoleEvent, RoleError> {
        if self.is_built_in() {
            return Err(RoleError::BuiltIn);
        }
        Ok(RoleEvent::Deleted { by })
    }

    /// Skips invariants: for repositories rebuilding persisted state only.
    pub fn restore(
        name: String,
        description: String,
        created_by: GrantedBy,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            name,
            description,
            created_by,
            created_at,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn is_built_in(&self) -> bool {
        BUILT_IN_ROLES.contains(&self.name.as_str())
    }

    pub fn created_by(&self) -> &GrantedBy {
        &self.created_by
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creating(name: &str, description: &str) -> Result<Role, RoleError> {
        Role::create(
            CreateRole {
                name: name.into(),
                description: description.into(),
            },
            GrantedBy::System,
            Utc::now(),
        )
        .map(|(role, _)| role)
    }

    #[test]
    fn a_name_is_trimmed_and_bounded() {
        assert_eq!(creating(" support ", "").unwrap().name(), "support");
        assert_eq!(creating("  ", ""), Err(RoleError::EmptyName));
        assert_eq!(
            creating(&"r".repeat(MAX_NAME_LEN + 1), ""),
            Err(RoleError::NameTooLong { max: MAX_NAME_LEN })
        );
        assert_eq!(
            creating("support", &"d".repeat(MAX_DESCRIPTION_LEN + 1)),
            Err(RoleError::DescriptionTooLong {
                max: MAX_DESCRIPTION_LEN
            })
        );
    }

    #[test]
    fn a_built_in_role_is_never_deleted() {
        for name in BUILT_IN_ROLES {
            let role = creating(name, "").unwrap();
            assert_eq!(role.delete(GrantedBy::System), Err(RoleError::BuiltIn));
        }
        assert!(
            creating("support", "")
                .unwrap()
                .delete(GrantedBy::System)
                .is_ok()
        );
    }
}
