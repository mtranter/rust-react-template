//! JWT Bearer authentication (spec §11).
//!
//! One `SecurityScheme` that flows into the OpenAPI spec (and thus the generated
//! client), with two verification modes behind a single type:
//!
//! * **Symmetric (HS256)** — a shared secret. Self-contained and can also
//!   *issue* tokens, which dev examples and integration tests rely on.
//! * **JWKS** — verify-only against an OIDC provider's published public keys,
//!   discovered from its `/.well-known/openid-configuration`. This is the
//!   production path: no user store, no token issuance — still just a seam.
//!
//! The server chooses the mode from config: set `jwt_issuer` for JWKS, otherwise
//! the HS256 secret is used. Everything beyond this remains a documented seam.

use app_error::{AppError, AppResult};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, decode_header, encode,
};
use poem::Request;
use poem_openapi::{SecurityScheme, auth::Bearer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Standard JWT claims used by the example guarded route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the authenticated principal's id.
    pub sub: String,
    /// Issued-at (unix seconds). Optional: some OIDC providers omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iat: Option<usize>,
    /// Expiry (unix seconds). Validated automatically.
    pub exp: usize,
}

/// Build a [`Validation`] for the given algorithms, optionally enforcing the
/// `iss` and `aud` claims. `exp` is always required; issuer/audience are checked
/// only when supplied (audience checking is disabled entirely when `None`).
fn validation(
    algorithms: &[Algorithm],
    issuer: Option<&str>,
    audience: Option<&str>,
) -> Validation {
    let mut validation = Validation::new(algorithms[0]);
    validation.algorithms = algorithms.to_vec();
    if let Some(iss) = issuer {
        validation.set_issuer(&[iss]);
    }
    match audience {
        Some(aud) => validation.set_audience(&[aud]),
        None => validation.validate_aud = false,
    }
    validation
}

/// Verifies (and, for the symmetric mode, issues) bearer tokens. Cloneable and
/// injected into request data by the server (`AddData`) so the security-scheme
/// checker can reach it.
#[derive(Clone)]
pub struct JwtVerifier(Arc<Mode>);

enum Mode {
    Symmetric(Symmetric),
    Jwks(Jwks),
}

struct Symmetric {
    decoding: DecodingKey,
    encoding: EncodingKey,
    validation: Validation,
}

struct Jwks {
    jwks_uri: String,
    http: reqwest::Client,
    validation: Validation,
    /// Signing keys by `kid`, refreshed on demand when a token presents an
    /// unknown key id (handles provider key rotation).
    keys: RwLock<HashMap<String, DecodingKey>>,
}

impl JwtVerifier {
    /// HS256 verifier from a shared secret, enforcing no issuer/audience. Can both
    /// verify and [`issue`] tokens. Convenience for the dev path and tests; use
    /// [`symmetric`] to also validate `iss`/`aud`.
    ///
    /// [`issue`]: JwtVerifier::issue
    /// [`symmetric`]: JwtVerifier::symmetric
    pub fn new(secret: &str) -> Self {
        Self::symmetric(secret, None, None)
    }

    /// HS256 verifier from a shared secret, optionally validating the `iss` and
    /// `aud` claims. Can both verify and [`issue`] tokens.
    ///
    /// [`issue`]: JwtVerifier::issue
    pub fn symmetric(secret: &str, issuer: Option<&str>, audience: Option<&str>) -> Self {
        Self(Arc::new(Mode::Symmetric(Symmetric {
            decoding: DecodingKey::from_secret(secret.as_bytes()),
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            validation: validation(&[Algorithm::HS256], issuer, audience),
        })))
    }

    /// Verify-only verifier built from an OIDC issuer's discovery document.
    ///
    /// Fetches `{issuer}/.well-known/openid-configuration`, then the advertised
    /// `jwks_uri`, and caches the signing keys by `kid`. Tokens are validated for
    /// signature, expiry, issuer, and (when `audience` is given) audience. Keys
    /// are refetched on demand when a token presents an unknown `kid`.
    pub async fn from_discovery(issuer: &str, audience: Option<&str>) -> AppResult<Self> {
        let http = reqwest::Client::new();
        let jwks_uri = discover_jwks_uri(&http, issuer).await?;
        let keys = fetch_keys(&http, &jwks_uri).await?;

        // Accept the asymmetric algorithms an OIDC provider might sign with, and
        // always pin the issuer to the discovery URL (audience is optional).
        let validation = validation(
            &[
                Algorithm::RS256,
                Algorithm::RS384,
                Algorithm::RS512,
                Algorithm::ES256,
                Algorithm::ES384,
            ],
            Some(issuer),
            audience,
        );

        Ok(Self(Arc::new(Mode::Jwks(Jwks {
            jwks_uri,
            http,
            validation,
            keys: RwLock::new(keys),
        }))))
    }

    /// Verify a bearer token, returning its claims. Any failure (bad signature,
    /// expired, malformed, unknown key, wrong issuer/audience) collapses to
    /// [`AppError::Unauthorized`].
    pub async fn verify(&self, token: &str) -> AppResult<Claims> {
        match &*self.0 {
            Mode::Symmetric(s) => decode::<Claims>(token, &s.decoding, &s.validation)
                .map(|data| data.claims)
                .map_err(|_| AppError::Unauthorized),
            Mode::Jwks(j) => j.verify(token).await,
        }
    }

    /// Dev/test helper: mint a token for `subject` valid for `ttl_secs`.
    ///
    /// Only the symmetric mode can issue (JWKS is verify-only); calling this on a
    /// JWKS verifier is a configuration error. No issuance backend ships (spec
    /// §11); this exists so examples and integration tests can exercise guarded
    /// routes.
    pub fn issue(&self, subject: impl Into<String>, ttl_secs: u64) -> AppResult<String> {
        let Mode::Symmetric(s) = &*self.0 else {
            return Err(AppError::internal(AuthError::NotIssuable));
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(AppError::internal)?
            .as_secs() as usize;
        let claims = Claims {
            sub: subject.into(),
            iat: Some(now),
            exp: now + ttl_secs as usize,
        };
        encode(&Header::new(Algorithm::HS256), &claims, &s.encoding).map_err(AppError::internal)
    }
}

impl Jwks {
    async fn verify(&self, token: &str) -> AppResult<Claims> {
        let header = decode_header(token).map_err(|_| AppError::Unauthorized)?;
        let kid = header.kid.ok_or(AppError::Unauthorized)?;

        let key = match self.key(&kid) {
            Some(key) => key,
            None => {
                // Unknown key id: the provider may have rotated. Refetch once and
                // retry — but only for a genuinely-unseen kid, so an expired or
                // forged token can't trigger a refetch storm.
                self.refresh().await?;
                self.key(&kid).ok_or(AppError::Unauthorized)?
            }
        };

        decode::<Claims>(token, &key, &self.validation)
            .map(|data| data.claims)
            .map_err(|_| AppError::Unauthorized)
    }

    fn key(&self, kid: &str) -> Option<DecodingKey> {
        self.keys.read().ok()?.get(kid).cloned()
    }

    async fn refresh(&self) -> AppResult<()> {
        let keys = fetch_keys(&self.http, &self.jwks_uri).await?;
        *self.keys.write().map_err(|_| AppError::Unauthorized)? = keys;
        Ok(())
    }
}

/// The `jwks_uri` field of the OIDC discovery document (other fields ignored).
#[derive(Deserialize)]
struct Discovery {
    jwks_uri: String,
}

async fn discover_jwks_uri(http: &reqwest::Client, issuer: &str) -> AppResult<String> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );
    let doc: Discovery = http
        .get(&url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(AppError::internal)?
        .json()
        .await
        .map_err(AppError::internal)?;
    Ok(doc.jwks_uri)
}

async fn fetch_keys(
    http: &reqwest::Client,
    jwks_uri: &str,
) -> AppResult<HashMap<String, DecodingKey>> {
    let set: JwkSet = http
        .get(jwks_uri)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(AppError::internal)?
        .json()
        .await
        .map_err(AppError::internal)?;

    let mut keys = HashMap::new();
    for jwk in &set.keys {
        // Keys without a `kid`, or that we can't turn into a decoding key
        // (unsupported type), are skipped rather than failing the whole set.
        if let (Some(kid), Ok(key)) = (jwk.common.key_id.clone(), DecodingKey::from_jwk(jwk)) {
            keys.insert(kid, key);
        }
    }
    if keys.is_empty() {
        return Err(AppError::internal(AuthError::EmptyJwks));
    }
    Ok(keys)
}

#[derive(Debug, thiserror::Error)]
enum AuthError {
    #[error("JWKS endpoint returned no usable keys")]
    EmptyJwks,
    #[error("cannot issue tokens: verifier is in JWKS (verify-only) mode")]
    NotIssuable,
}

/// JWT Bearer security scheme. Apply as an operation parameter to guard a route:
/// the extracted [`Claims`] identify the caller.
#[derive(SecurityScheme)]
#[oai(ty = "bearer", checker = "jwt_checker")]
pub struct JwtAuth(pub Claims);

async fn jwt_checker(req: &Request, bearer: Bearer) -> Option<Claims> {
    let verifier = req.data::<JwtVerifier>()?;
    verifier.verify(&bearer.token).await.ok()
}
