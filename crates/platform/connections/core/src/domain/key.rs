use super::ConnectionError;
use std::fmt;

const MAX_KEY_LEN: usize = 64;

/// A Connection's key, as its module declares it (`crm`): unique across the app.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionKey(String);

impl ConnectionKey {
    pub fn parse(raw: &str) -> Result<Self, ConnectionError> {
        let valid = raw.len() <= MAX_KEY_LEN
            && raw.starts_with(|c: char| c.is_ascii_lowercase())
            && raw
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        if valid {
            Ok(Self(raw.to_owned()))
        } else {
            Err(ConnectionError::InvalidKey(raw.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConnectionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_lowercase_words_from_a_letter() {
        for good in ["crm", "billing-v2", "erp_eu"] {
            assert!(ConnectionKey::parse(good).is_ok(), "{good}");
        }
        for bad in ["", "CRM", "2crm", "crm api", "-crm", &"x".repeat(65)] {
            assert!(ConnectionKey::parse(bad).is_err(), "{bad}");
        }
    }
}
