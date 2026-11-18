//! JWT Bearer authentication (ADR 0025): HS256 (dev/tests; can also issue) or
//! verify-only JWKS discovered from an OIDC issuer (prod).

use app_authz::{Authorizer, Principal, USER};
use app_error::{AppError, AppResult};
use async_trait::async_trait;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, decode_header, encode,
};
use poem::{Request, http::header};
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
    /// OIDC's standard profile claims, when the provider puts them in the access token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

impl Claims {
    /// Of `subject`, issued now and expiring in `ttl_secs`, with no profile.
    pub fn lasting(subject: impl Into<String>, ttl_secs: u64) -> AppResult<Self> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(AppError::internal)?
            .as_secs() as usize;
        Ok(Self {
            sub: subject.into(),
            iat: Some(now),
            exp: now + ttl_secs as usize,
            name: None,
            email: None,
        })
    }

    pub fn named(self, name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..self
        }
    }

    pub fn with_email(self, email: impl Into<String>) -> Self {
        Self {
            email: Some(email.into()),
            ..self
        }
    }
}

/// Turns a verified token's claims into who is acting (ADR 0040). Held in request data as an
/// `Arc<dyn PrincipalResolver>`; without one, the [`Authorizer`] in request data is used.
#[async_trait]
pub trait PrincipalResolver: Send + Sync {
    async fn resolve(&self, claims: &Claims) -> AppResult<Principal>;
}

/// The default resolver: the user's id is the token's `sub`, their roles `user` and those
/// assigned to the subject in the rule set in force (kept by `access`), so a request reads no
/// store for them. A `roles` claim in the token is ignored.
#[async_trait]
impl PrincipalResolver for Authorizer {
    async fn resolve(&self, claims: &Claims) -> AppResult<Principal> {
        Ok(self.principal(&claims.sub))
    }
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

    /// Dev/test only: no issuance backend ships (ADR 0025). Fails in JWKS mode.
    pub fn issue(&self, subject: impl Into<String>, ttl_secs: u64) -> AppResult<String> {
        self.sign(&Claims::lasting(subject, ttl_secs)?)
    }

    /// Dev/test only, as [`Self::issue`], for claims carrying a profile.
    pub fn sign(&self, claims: &Claims) -> AppResult<String> {
        let Mode::Symmetric(s) = &*self.0 else {
            return Err(AppError::internal(AuthError::NotIssuable));
        };
        encode(&Header::new(Algorithm::HS256), claims, &s.encoding).map_err(AppError::internal)
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

/// JWT Bearer security scheme: the token verified, then resolved to the [`Principal`] acting. A
/// Module's operations take its `Caller` instead, which passes its gate too.
#[derive(SecurityScheme)]
#[oai(ty = "bearer", checker = "jwt_checker")]
pub struct JwtAuth(pub Principal);

/// The token of `req`'s `Authorization: Bearer` header, if it has one.
pub(crate) fn bearer_token(req: &Request) -> Option<&str> {
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token.trim())
}

/// The claims of `req`'s bearer token, if it has a valid one.
pub(crate) async fn verified_caller(req: &Request) -> Option<Claims> {
    let (token, verifier) = (bearer_token(req)?, req.data::<JwtVerifier>()?);
    verifier.verify(token).await.ok()
}

async fn jwt_checker(req: &Request, bearer: Bearer) -> Option<Principal> {
    let verifier = req.data::<JwtVerifier>()?;
    let claims = verifier.verify(&bearer.token).await.ok()?;
    let resolved = match (
        req.data::<Arc<dyn PrincipalResolver>>(),
        req.data::<Arc<Authorizer>>(),
    ) {
        (Some(resolver), _) => resolver.resolve(&claims).await,
        (None, Some(authorizer)) => authorizer.resolve(&claims).await,
        (None, None) => Ok(Principal::user(claims.sub, [USER])),
    };
    resolved.ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_authz::RuleSet;
    use poem::test::TestClient;
    use poem::{EndpointExt, Route, handler};
    use serde_json::json;

    #[handler]
    async fn roles(req: &Request) -> String {
        let token = bearer_token(req).unwrap_or_default().to_owned();
        match jwt_checker(req, Bearer { token }).await {
            Some(Principal::User { roles, .. }) => roles.join(","),
            _ => "none".to_owned(),
        }
    }

    async fn roles_of(token: &str, authorizer: Arc<Authorizer>) -> String {
        let app = Route::new()
            .at("/", roles)
            .data(JwtVerifier::new("secret"))
            .data(authorizer);
        TestClient::new(app)
            .get("/")
            .header("authorization", format!("Bearer {token}"))
            .send()
            .await
            .0
            .into_body()
            .into_string()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_roles_claim_in_the_token_gains_nothing() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let claiming_admin = encode(
            &Header::new(Algorithm::HS256),
            &json!({ "sub": "ada", "exp": now + 60, "roles": ["admin", "support"] }),
            &EncodingKey::from_secret(b"secret"),
        )
        .unwrap();

        let authorizer = Arc::new(Authorizer::new(RuleSet::default()));

        assert_eq!(roles_of(&claiming_admin, authorizer).await, "user");
    }

    #[tokio::test]
    async fn a_subject_has_user_and_the_roles_assigned_to_it() {
        let token = JwtVerifier::new("secret").issue("ada", 60).unwrap();
        let authorizer = Arc::new(Authorizer::new(
            RuleSet::default().assigning([("ada".to_owned(), "support".to_owned())]),
        ));

        assert_eq!(roles_of(&token, authorizer).await, "user,support");
    }
}
