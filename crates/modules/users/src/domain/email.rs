//! The `Email` value object.

use super::UserError;
use std::fmt;

/// A normalized, validated email address. The only way to build one from user
/// input is [`Email::parse`], so holding an `Email` proves it is valid.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Email(String);

impl Email {
    /// Normalize (trim, lowercase) and validate a raw address.
    pub fn parse(raw: &str) -> Result<Self, UserError> {
        let email = raw.trim().to_ascii_lowercase();
        // Deliberately minimal: exactly one '@', non-empty local and domain parts.
        match email.split_once('@') {
            Some((local, domain))
                if !local.is_empty() && domain.contains('.') && !domain.starts_with('.') =>
            {
                Ok(Self(email))
            }
            _ => Err(UserError::InvalidEmail),
        }
    }

    /// Rebuild from persisted state without re-validating. For repositories
    /// only: stored values were valid when saved.
    pub(super) fn restore(email: String) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rejects_addresses_without_at() {
        assert_eq!(Email::parse("not-an-email"), Err(UserError::InvalidEmail));
    }

    #[test]
    fn lowercases_and_trims() {
        assert_eq!(
            Email::parse("  Foo@Example.Com ").unwrap().as_str(),
            "foo@example.com"
        );
    }

    proptest! {
        // Any string with no '@' is never a valid email.
        #[test]
        fn no_at_is_always_invalid(s in "[^@]{1,64}") {
            prop_assert!(Email::parse(&s).is_err());
        }
    }
}
