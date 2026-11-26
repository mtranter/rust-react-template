use super::ConnectionError;
use chrono::{DateTime, Utc};
use std::fmt;

const MAX_SECRET_LEN: usize = 8 * 1024;

/// A secret as stored: its value sealed with AES-256-GCM under a data key of its own, and that
/// key wrapped by the master key `key_version` names (envelope encryption, ADR 0050).
#[derive(Clone, PartialEq, Eq)]
pub struct SealedSecret {
    /// The nonce, then the ciphertext and its tag.
    pub ciphertext: Vec<u8>,
    pub wrapped_key: Vec<u8>,
    pub key_version: String,
}

impl fmt::Debug for SealedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SealedSecret")
            .field("key_version", &self.key_version)
            .finish_non_exhaustive()
    }
}

/// A secret a Connection holds, and who set it when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Secret {
    pub sealed: SealedSecret,
    pub set_at: DateTime<Utc>,
    pub set_by: String,
}

/// `raw` as a secret's value: kept exactly as given, as a key's spaces may be part of it.
pub fn secret_value(raw: &str) -> Result<&str, ConnectionError> {
    if raw.is_empty() {
        return Err(ConnectionError::EmptySecret);
    }
    if raw.len() > MAX_SECRET_LEN {
        return Err(ConnectionError::SecretTooLong {
            max: MAX_SECRET_LEN,
        });
    }
    Ok(raw)
}
