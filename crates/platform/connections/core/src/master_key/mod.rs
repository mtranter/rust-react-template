//! The first [`KeyProvider`] (ADR 0050): one master key, from `CONNECTIONS_MASTER_KEY`.

use crate::application::{DataKey, KEY_LEN, KeyError, KeyProvider, WrappedKey, decrypt, encrypt};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ring::digest::{SHA256, digest};

const AAD: &[u8] = b"connections data key";

/// Wraps data keys with AES-256-GCM under one 32-byte master key.
pub struct EnvKeyProvider {
    key: [u8; KEY_LEN],
    version: String,
}

impl EnvKeyProvider {
    /// `encoded` is the key in standard base64 (`openssl rand -base64 32`).
    pub fn from_base64(encoded: &str) -> Result<Self, String> {
        let bytes = STANDARD
            .decode(encoded.trim())
            .map_err(|_| "CONNECTIONS_MASTER_KEY isn't base64".to_owned())?;
        let key: [u8; KEY_LEN] = bytes.try_into().map_err(|bytes: Vec<u8>| {
            format!(
                "CONNECTIONS_MASTER_KEY is {} bytes, not {KEY_LEN}",
                bytes.len()
            )
        })?;
        Ok(Self::new(key))
    }

    /// Its version names the key without revealing it: the start of its SHA-256, so a secret
    /// sealed under another key is told apart before it is tried.
    pub fn new(key: [u8; KEY_LEN]) -> Self {
        let hash = digest(&SHA256, &key);
        let fingerprint: String = hash.as_ref()[..4]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Self {
            key,
            version: format!("env:{fingerprint}"),
        }
    }

    /// `None` when `CONNECTIONS_MASTER_KEY` is unset or empty.
    pub fn from_env() -> Result<Option<Self>, String> {
        match std::env::var("CONNECTIONS_MASTER_KEY") {
            Ok(key) if !key.trim().is_empty() => Self::from_base64(&key).map(Some),
            _ => Ok(None),
        }
    }
}

#[async_trait::async_trait]
impl KeyProvider for EnvKeyProvider {
    async fn wrap(&self, key: &DataKey) -> Result<WrappedKey, KeyError> {
        Ok(WrappedKey {
            key_version: self.version.clone(),
            bytes: encrypt(&self.key, AAD, key.as_bytes())?,
        })
    }

    async fn unwrap(&self, wrapped: &WrappedKey) -> Result<DataKey, KeyError> {
        if wrapped.key_version != self.version {
            return Err(KeyError::WrongKey);
        }
        let bytes = decrypt(&self.key, AAD, &wrapped.bytes)?;
        let key: [u8; KEY_LEN] = bytes.try_into().map_err(|_| KeyError::WrongKey)?;
        Ok(DataKey::from_bytes(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_32_bytes_of_base64() {
        assert!(EnvKeyProvider::from_base64(&STANDARD.encode([1; 32])).is_ok());
        assert!(EnvKeyProvider::from_base64(&STANDARD.encode([1; 16])).is_err());
        assert!(EnvKeyProvider::from_base64("not base64!").is_err());
    }

    #[tokio::test]
    async fn a_data_key_unwraps_under_the_key_that_wrapped_it_only() {
        let key = DataKey::from_bytes([9; KEY_LEN]);
        let wrapped = EnvKeyProvider::new([1; KEY_LEN]).wrap(&key).await.unwrap();

        let unwrapped = EnvKeyProvider::new([1; KEY_LEN])
            .unwrap(&wrapped)
            .await
            .unwrap();
        assert_eq!(unwrapped.as_bytes(), key.as_bytes());
        assert!(
            EnvKeyProvider::new([2; KEY_LEN])
                .unwrap(&wrapped)
                .await
                .is_err()
        );
    }
}
