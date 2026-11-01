//! JWT Bearer authentication (spec §11): HS256 (dev/tests; can also issue) or
//! verify-only JWKS discovered from an OIDC issuer (prod).

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    // Optional: some OIDC providers omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iat: Option<usize>,
    pub exp: usize,
}

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

/// Held in request data so `jwt_checker` can reach it.
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
    keys: RwLock<HashMap<String, DecodingKey>>,
}

impl JwtVerifier {
    pub fn new(secret: &str) -> Self {
        Self::symmetric(secret, None, None)
    }

    pub fn symmetric(secret: &str, issuer: Option<&str>, audience: Option<&str>) -> Self {
        Self(Arc::new(Mode::Symmetric(Symmetric {
            decoding: DecodingKey::from_secret(secret.as_bytes()),
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            validation: validation(&[Algorithm::HS256], issuer, audience),
        })))
    }

    pub async fn from_discovery(issuer: &str, audience: Option<&str>) -> AppResult<Self> {
        let http = reqwest::Client::new();
        let jwks_uri = discover_jwks_uri(&http, issuer).await?;
        let keys = fetch_keys(&http, &jwks_uri).await?;

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

    pub async fn verify(&self, token: &str) -> AppResult<Claims> {
        match &*self.0 {
            Mode::Symmetric(s) => decode::<Claims>(token, &s.decoding, &s.validation)
                .map(|data| data.claims)
                .map_err(|_| AppError::Unauthorized),
            Mode::Jwks(j) => j.verify(token).await,
        }
    }

    /// Dev/test only: no issuance backend ships (spec §11). Fails in JWKS mode.
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
                // Unknown kid: the provider may have rotated its keys.
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
        // Skip unusable keys rather than failing the whole set.
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
