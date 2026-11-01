use super::{ChangeDisplayName, Email, RegisterUser, UserError, UserId};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    id: UserId,
    email: Email,
    display_name: String,
    created_at: DateTime<Utc>,
}

impl User {
    /// Takes the id and `now` so the domain stays deterministic: the service
    /// passes fresh values, seeds pass well-known ones.
    pub fn register(id: UserId, cmd: RegisterUser, now: DateTime<Utc>) -> Result<Self, UserError> {
        Ok(Self {
            id,
            email: Email::parse(&cmd.email)?,
            display_name: display_name(&cmd.display_name)?,
            created_at: now,
        })
    }

    pub fn change_display_name(&mut self, cmd: ChangeDisplayName) -> Result<(), UserError> {
        self.display_name = display_name(&cmd.display_name)?;
        Ok(())
    }

    /// Skips invariants: for repositories only.
    pub fn restore(
        id: UserId,
        email: String,
        display_name: String,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            email: Email::restore(email),
            display_name,
            created_at,
        }
    }

    pub fn id(&self) -> UserId {
        self.id
    }

    pub fn email(&self) -> &Email {
        &self.email
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

fn display_name(raw: &str) -> Result<String, UserError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(UserError::EmptyDisplayName);
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> User {
        User::register(
            UserId::new(),
            RegisterUser {
                email: "Alice@Example.com".into(),
                display_name: "  Alice ".into(),
            },
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn register_normalizes_input() {
        let user = alice();
        assert_eq!(user.email().as_str(), "alice@example.com");
        assert_eq!(user.display_name(), "Alice");
    }

    #[test]
    fn register_rejects_blank_display_name() {
        let result = User::register(
            UserId::new(),
            RegisterUser {
                email: "a@example.com".into(),
                display_name: "   ".into(),
            },
            Utc::now(),
        );
        assert_eq!(result, Err(UserError::EmptyDisplayName));
    }

    #[test]
    fn rejected_change_leaves_aggregate_unchanged() {
        let mut user = alice();
        let before = user.clone();
        let result = user.change_display_name(ChangeDisplayName {
            user_id: user.id(),
            display_name: " ".into(),
        });
        assert_eq!(result, Err(UserError::EmptyDisplayName));
        assert_eq!(user, before);
    }
}
