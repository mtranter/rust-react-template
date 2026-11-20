//! JWT Bearer authentication (ADR 0025): HS256 (dev/tests; can also issue) or
//! verify-only JWKS discovered from an OIDC issuer (prod).

use app_authz::{Actor, Authorizer, Principal, USER};
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
    /// The agent acting for `sub` (RFC 8693 §4.1), when the identity provider exchanged the
    /// user's token for one naming it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub act: Option<Act>,
    /// The client the token was issued to (RFC 9068); `azp` where a provider names it so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub azp: Option<String>,
}

/// An `act` claim: the acting party, and in its own `act` the one it acts for in turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Act {
    pub sub: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub act: Option<Box<Act>>,
}

impl Act {
    /// `subs`, outermost first, each acting for the next.
    pub fn chain(subs: impl IntoIterator<Item = impl Into<String>>) -> Option<Self> {
        let subs: Vec<String> = subs.into_iter().map(Into::into).collect();
        subs.into_iter().rev().fold(None, |inner, sub| {
            Some(Act {
                sub,
                act: inner.map(Box::new),
            })
        })
    }

    fn actor(&self) -> Actor {
        let chain = std::iter::successors(self.act.as_deref(), |act| act.act.as_deref())
            .map(|act| act.sub.clone());
        Actor::new(self.sub.clone()).behind(chain)
    }
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
            act: None,
            client_id: None,
            azp: None,
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

    /// Acted on by `agents`, outermost first, as a token exchange names them.
    pub fn acted_by(self, agents: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            act: Act::chain(agents),
            ..self
        }
    }

    pub fn from_client(self, client_id: impl Into<String>) -> Self {
        Self {
            client_id: Some(client_id.into()),
            ..self
        }
    }

    pub fn authorized_party(self, azp: impl Into<String>) -> Self {
        Self {
            azp: Some(azp.into()),
            ..self
        }
    }

    /// The agent acting: the outermost `act`, else the token's client when it isn't one of the
    /// app's own (`is_first_party`). A client-credentials token's client is its subject, so an
    /// agent acting for no one is its own actor.
    pub fn actor(&self, is_first_party: impl Fn(&str) -> bool) -> Option<Actor> {
        if let Some(act) = &self.act {
            return Some(act.actor());
        }
        self.client_id
            .as_deref()
            .or(self.azp.as_deref())
            .filter(|client| !is_first_party(client))
            .map(Actor::new)
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
/// store for them, and the agent acting [`Claims::actor`]. A `roles` claim in the token is
/// ignored.
#[async_trait]
impl PrincipalResolver for Authorizer {
    async fn resolve(&self, claims: &Claims) -> AppResult<Principal> {
        let actor = claims.actor(|client| self.is_first_party_client(client));
        Ok(self.principal(&claims.sub).acting(actor))
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

        if !self.validation.algorithms.contains(&header.alg) {
            return Err(AppError::Unauthorized);
        }
        // jsonwebtoken refuses a validation listing algorithms of another
        // family than the key's (RSA beside EC), so it names only the token's.
        let validation = Validation {
            algorithms: vec![header.alg],
            ..self.validation.clone()
        };
        decode::<Claims>(token, &key, &validation)
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
    crate::trace::record_subject(req, &claims.sub);
    let resolved = match (
        req.data::<Arc<dyn PrincipalResolver>>(),
        req.data::<Arc<Authorizer>>(),
    ) {
        (Some(resolver), _) => resolver.resolve(&claims).await,
        (None, Some(authorizer)) => authorizer.resolve(&claims).await,
        // With no Authorizer the app's own clients are unknown, so only an `act` names an agent.
        (None, None) => Ok(Principal::user(&claims.sub, [USER]).acting(claims.actor(|_| true))),
    };
    resolved.ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_authz::{Actor, RuleSet};
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

    const SPA: &str = "spa";

    async fn actor_of(claims: Claims) -> Option<Actor> {
        let authorizer =
            Authorizer::new(RuleSet::default()).with_first_party_clients([SPA.to_owned()]);
        let token = JwtVerifier::new("secret").sign(&claims).unwrap();
        let claims = JwtVerifier::new("secret").verify(&token).await.unwrap();
        authorizer.resolve(&claims).await.unwrap().actor().cloned()
    }

    fn of(subject: &str) -> Claims {
        Claims::lasting(subject, 60).unwrap()
    }

    #[tokio::test]
    async fn a_users_own_token_through_the_apps_client_names_no_agent() {
        assert_eq!(actor_of(of("ada").from_client(SPA)).await, None);
        assert_eq!(actor_of(of("ada").authorized_party(SPA)).await, None);
        assert_eq!(actor_of(of("ada")).await, None);
    }

    #[tokio::test]
    async fn a_token_exchanged_for_an_agent_names_the_outermost_act_with_those_behind_it() {
        let claims = of("ada")
            .from_client(SPA)
            .acted_by(["planner", "researcher", "fetcher"]);

        assert_eq!(
            actor_of(claims).await,
            Some(Actor::new("planner").behind(["researcher", "fetcher"]))
        );
        assert_eq!(
            actor_of(of("ada").acted_by(["planner"])).await,
            Some(Actor::new("planner"))
        );
    }

    #[tokio::test]
    async fn a_token_another_client_obtained_names_that_client() {
        assert_eq!(
            actor_of(of("ada").from_client("claude")).await,
            Some(Actor::new("claude"))
        );
        assert_eq!(
            actor_of(of("ada").authorized_party("claude")).await,
            Some(Actor::new("claude"))
        );
        assert_eq!(
            actor_of(of("ada").from_client("claude").authorized_party(SPA)).await,
            Some(Actor::new("claude")),
            "client_id is read before azp"
        );
    }

    #[tokio::test]
    async fn a_client_credentials_token_is_an_agent_acting_as_itself() {
        let authorizer = Authorizer::new(
            RuleSet::default().assigning([("reconciler".to_owned(), "support".to_owned())]),
        )
        .with_first_party_clients([SPA.to_owned()]);

        let principal = authorizer
            .resolve(&of("reconciler").from_client("reconciler"))
            .await
            .unwrap();

        assert_eq!(principal.id(), Some("reconciler"));
        assert_eq!(principal.actor(), Some(&Actor::new("reconciler")));
        let Principal::User { roles: held, .. } = principal else {
            unreachable!("a token resolves to a user")
        };
        assert_eq!(held, ["user", "support"]);
    }

    #[handler]
    async fn actor(req: &Request) -> String {
        let token = bearer_token(req).unwrap_or_default().to_owned();
        let principal = jwt_checker(req, Bearer { token }).await;
        format!("{:?}", principal.as_ref().and_then(Principal::actor))
    }

    #[tokio::test]
    async fn with_no_authorizer_only_an_act_claim_names_an_agent() {
        let verifier = JwtVerifier::new("secret");
        let app = Route::new().at("/", actor).data(verifier.clone());
        let client = TestClient::new(app);
        let actor_named_by = |claims: Claims| {
            let token = verifier.sign(&claims).unwrap();
            let client = &client;
            async move {
                client
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
        };

        assert_eq!(actor_named_by(of("ada").from_client(SPA)).await, "None");
        assert_eq!(
            actor_named_by(of("ada").from_client(SPA).acted_by(["planner"])).await,
            format!("{:?}", Some(&Actor::new("planner")))
        );
    }

    #[test]
    fn nested_act_claims_deserialise_from_a_providers_token() {
        let claims: Claims = serde_json::from_value(json!({
            "sub": "ada",
            "exp": 1,
            "azp": "spa",
            "act": { "sub": "planner", "iss": "https://idp", "act": { "sub": "fetcher" } },
        }))
        .unwrap();

        assert_eq!(claims.azp.as_deref(), Some("spa"));
        assert_eq!(claims.client_id, None);
        assert_eq!(claims.act, Act::chain(["planner", "fetcher"]));
    }
}
