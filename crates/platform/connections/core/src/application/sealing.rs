//! Envelope encryption (ADR 0050): each secret is sealed with AES-256-GCM under a fresh data key,
//! and only that data key goes to the [`KeyProvider`], which wraps it under a master key the
//! database never sees. A KMS adapter is another `KeyProvider`, and changes neither this format
//! nor the module's API.

use crate::domain::SealedSecret;
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use std::fmt;

pub const KEY_LEN: usize = 32;

/// A secret's own AES-256 key, in the clear only while it seals or opens one.
pub struct DataKey([u8; KEY_LEN]);

impl DataKey {
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    fn generate() -> Result<Self, KeyError> {
        Ok(Self(random()?))
    }
}

impl fmt::Debug for DataKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DataKey(..)")
    }
}

/// A data key as a [`KeyProvider`] wrapped it, and which of its master keys did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrappedKey {
    pub key_version: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// The key that sealed it isn't the one given, or it was altered.
    #[error("the secret doesn't open with this key")]
    WrongKey,
    #[error("the key provider failed: {0}")]
    Provider(String),
}

/// Wraps and unwraps data keys under a master key: the one place the master key is held.
#[async_trait::async_trait]
pub trait KeyProvider: Send + Sync {
    async fn wrap(&self, key: &DataKey) -> Result<WrappedKey, KeyError>;
    async fn unwrap(&self, wrapped: &WrappedKey) -> Result<DataKey, KeyError>;
}

/// `context` is bound into the ciphertext (its AAD), so a sealed secret opens only where it was
/// sealed: copied to another Connection's or another secret's row, it fails to open.
pub async fn seal(
    provider: &dyn KeyProvider,
    context: &str,
    plaintext: &str,
) -> Result<SealedSecret, KeyError> {
    let key = DataKey::generate()?;
    let ciphertext = encrypt(key.as_bytes(), context.as_bytes(), plaintext.as_bytes())?;
    let wrapped = provider.wrap(&key).await?;
    Ok(SealedSecret {
        ciphertext,
        wrapped_key: wrapped.bytes,
        key_version: wrapped.key_version,
    })
}

/// The plaintext `sealed` was sealed from with `context`.
pub async fn open(
    provider: &dyn KeyProvider,
    context: &str,
    sealed: &SealedSecret,
) -> Result<String, KeyError> {
    let key = provider
        .unwrap(&WrappedKey {
            key_version: sealed.key_version.clone(),
            bytes: sealed.wrapped_key.clone(),
        })
        .await?;
    let plaintext = decrypt(key.as_bytes(), context.as_bytes(), &sealed.ciphertext)?;
    String::from_utf8(plaintext).map_err(|_| KeyError::WrongKey)
}

/// The nonce, then the ciphertext and its tag. A random 96-bit nonce is safe here: every key
/// seals only once.
pub fn encrypt(key: &[u8; KEY_LEN], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, KeyError> {
    let nonce: [u8; NONCE_LEN] = random()?;
    let mut sealed = plaintext.to_vec();
    cipher(key)
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad),
            &mut sealed,
        )
        .map_err(|_| KeyError::Provider("AES-GCM refused to seal".into()))?;
    Ok([nonce.as_slice(), &sealed].concat())
}

pub fn decrypt(key: &[u8; KEY_LEN], aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, KeyError> {
    if sealed.len() < NONCE_LEN {
        return Err(KeyError::WrongKey);
    }
    let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
    let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| KeyError::WrongKey)?;
    let mut opened = ciphertext.to_vec();
    let plaintext = cipher(key)
        .open_in_place(nonce, Aad::from(aad), &mut opened)
        .map_err(|_| KeyError::WrongKey)?;
    Ok(plaintext.to_vec())
}

fn cipher(key: &[u8; KEY_LEN]) -> LessSafeKey {
    LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).expect("an AES-256 key is 32 bytes"))
}

fn random<const N: usize>() -> Result<[u8; N], KeyError> {
    let mut bytes = [0; N];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| KeyError::Provider("no randomness from the system".into()))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wraps under `master` as the env adapter does, without the environment.
    struct Master([u8; KEY_LEN]);

    #[async_trait::async_trait]
    impl KeyProvider for Master {
        async fn wrap(&self, key: &DataKey) -> Result<WrappedKey, KeyError> {
            Ok(WrappedKey {
                key_version: "test".into(),
                bytes: encrypt(&self.0, b"data key", key.as_bytes())?,
            })
        }

        async fn unwrap(&self, wrapped: &WrappedKey) -> Result<DataKey, KeyError> {
            let bytes = decrypt(&self.0, b"data key", &wrapped.bytes)?;
            Ok(DataKey::from_bytes(
                bytes.try_into().map_err(|_| KeyError::WrongKey)?,
            ))
        }
    }

    const SECRET: &str = "sk_live_0123456789abcdef";

    #[tokio::test]
    async fn a_sealed_secret_opens_with_its_key_and_context_only() {
        let master = Master([7; KEY_LEN]);
        let sealed = seal(&master, "crm/api_key", SECRET).await.unwrap();

        assert_eq!(open(&master, "crm/api_key", &sealed).await.unwrap(), SECRET);
        assert!(matches!(
            open(&Master([8; KEY_LEN]), "crm/api_key", &sealed).await,
            Err(KeyError::WrongKey)
        ));
        assert!(matches!(
            open(&master, "erp/api_key", &sealed).await,
            Err(KeyError::WrongKey)
        ));
    }

    #[tokio::test]
    async fn neither_the_ciphertext_nor_the_wrapped_key_holds_the_secret() {
        let master = Master([7; KEY_LEN]);
        let sealed = seal(&master, "crm/api_key", SECRET).await.unwrap();
        let again = seal(&master, "crm/api_key", SECRET).await.unwrap();

        for stored in [&sealed.ciphertext, &sealed.wrapped_key] {
            assert!(
                !stored
                    .windows(SECRET.len())
                    .any(|window| window == SECRET.as_bytes())
            );
        }
        assert_ne!(sealed.ciphertext, again.ciphertext);
    }
}
