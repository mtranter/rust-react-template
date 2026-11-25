//! An OIDC provider for end-to-end checks: discovery, JWKS, an authorize
//! endpoint that approves straight away as one named subject, a token endpoint
//! for the authorization code with PKCE, and end-session. Like Cognito's MCP
//! client support, it also registers clients dynamically (RFC 7591, at
//! `/oauth2/register`) and accepts client ID metadata document URLs as client IDs. Its RS256 key is made
//! at start, so its tokens verify only against the JWKS it serves. Beside it, a
//! stand-in for the Cognito user pool API invitations go through ([`UserPool`]).

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::jwk::{Jwk, JwkSet, PublicKeyUse};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use poem::{
    Endpoint, EndpointExt, IntoResponse, Response, Route, get, handler,
    http::StatusCode,
    middleware::Cors,
    post,
    web::{Data, Form, Json, Query, Redirect},
};
use rand::{Rng, thread_rng};
use rsa::{RsaPrivateKey, pkcs1::EncodeRsaPrivateKey};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

const KEY_ID: &str = "fake-oidc";
const TOKEN_TTL_SECS: u64 = 3600;
/// The scope `/authorize` grants when the client asks for none.
pub const DEFAULT_SCOPE: &str = "openid profile email";
/// The scope Cognito requires of the bearer token on a protected registration.
pub const REGISTER_SCOPE: &str = "aws.cognito/dcr/register";
/// The largest client ID metadata document fetched, in bytes.
const CIMD_MAX_BYTES: usize = 5 * 1024;

/// Who every sign-in approves as.
#[derive(Debug, Clone)]
pub struct Subject {
    pub sub: String,
    pub name: String,
    pub email: String,
}

pub struct Provider {
    issuer: String,
    client_id: String,
    subject: Subject,
    key: EncodingKey,
    decoding: DecodingKey,
    jwks: JwkSet,
    codes: Mutex<HashMap<String, Approval>>,
    /// Clients registered through `/oauth2/register`, by ID, with their redirect URIs.
    registered: Mutex<HashMap<String, Vec<String>>>,
    registration_requires_token: bool,
    http: reqwest::Client,
}

/// What an authorization code was issued for, checked when it is redeemed.
struct Approval {
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    nonce: Option<String>,
    scope: String,
    resource: Option<String>,
}

impl Provider {
    /// `issuer` is where it is reached, without a trailing `/`: tokens carry it as `iss`.
    /// `client_id` is the configured SPA client, which may redirect anywhere.
    /// Registration through `/oauth2/register` starts open.
    pub fn new(issuer: impl Into<String>, client_id: impl Into<String>, subject: Subject) -> Self {
        let der = RsaPrivateKey::new(&mut thread_rng(), 2048)
            .and_then(|key| Ok(key.to_pkcs1_der()?))
            .expect("an RSA key");
        let key = EncodingKey::from_rsa_der(der.as_bytes());
        let mut jwk = Jwk::from_encoding_key(&key, Algorithm::RS256).expect("the key's JWK");
        jwk.common.key_id = Some(KEY_ID.to_string());
        jwk.common.public_key_use = Some(PublicKeyUse::Signature);
        let decoding = DecodingKey::from_jwk(&jwk).expect("the key's public half");
        Self {
            issuer: issuer.into(),
            client_id: client_id.into(),
            subject,
            key,
            decoding,
            jwks: JwkSet { keys: vec![jwk] },
            codes: Mutex::new(HashMap::new()),
            registered: Mutex::new(HashMap::new()),
            registration_requires_token: false,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("an HTTP client"),
        }
    }

    /// Whether `/oauth2/register` requires a bearer access token from this provider
    /// carrying the scope [`REGISTER_SCOPE`], like a Cognito pool with protected
    /// registration. Off by default: registration is open.
    pub fn require_registration_token(mut self, required: bool) -> Self {
        self.registration_requires_token = required;
        self
    }

    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// An access token as `/token` would issue for `client_id` granted `scope`,
    /// with `resource` as its `aud` when given: for tests that skip the redirect.
    pub fn access_token(&self, client_id: &str, scope: &str, resource: Option<&str>) -> String {
        self.sign(&self.access_claims(client_id, scope, resource))
    }

    /// The provider's routes, open to any origin: the SPA's browser calls them.
    pub fn endpoint(self: Arc<Self>) -> impl Endpoint {
        Route::new()
            .at("/.well-known/openid-configuration", get(discovery))
            .at("/.well-known/oauth-authorization-server", get(discovery))
            .at("/jwks", get(jwks))
            .at("/authorize", get(authorize))
            .at("/token", post(token))
            .at("/oauth2/register", post(register))
            .at("/end-session", get(end_session))
            .nest("/cognito", Arc::new(UserPool::default()).endpoint())
            .with(Cors::new())
            .data(self)
    }

    fn sign(&self, claims: &Value) -> String {
        let header = Header {
            kid: Some(KEY_ID.to_string()),
            ..Header::new(Algorithm::RS256)
        };
        encode(&header, claims, &self.key).expect("signs")
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("after 1970")
            .as_secs()
    }

    /// Cognito-shaped access claims: `client_id`, a space separated `scope`, and
    /// `token_use`. `aud` is the RFC 8707 resource when one was asked for, else the
    /// SPA client's ID for that client alone (as before), else absent, as on Cognito.
    fn access_claims(&self, client_id: &str, scope: &str, resource: Option<&str>) -> Value {
        let now = Self::now();
        let Subject { sub, name, email } = &self.subject;
        let mut claims = json!({
            "iss": self.issuer,
            "sub": sub,
            "iat": now,
            "exp": now + TOKEN_TTL_SECS,
            "name": name,
            "email": email,
            "client_id": client_id,
            "scope": scope,
            "token_use": "access",
        });
        if let Some(resource) = resource {
            claims["aud"] = json!(resource);
        } else if client_id == self.client_id {
            claims["aud"] = json!(client_id);
        }
        claims
    }

    fn tokens(&self, approval: Approval) -> Value {
        let now = Self::now();
        let Subject { sub, name, email } = &self.subject;
        let mut id_claims = json!({
            "iss": self.issuer,
            "sub": sub,
            "aud": approval.client_id,
            "iat": now,
            "exp": now + TOKEN_TTL_SECS,
            "name": name,
            "email": email,
            "token_use": "id",
        });
        if let Some(nonce) = approval.nonce {
            id_claims["nonce"] = json!(nonce);
        }
        let access = self.access_claims(
            &approval.client_id,
            &approval.scope,
            approval.resource.as_deref(),
        );
        json!({
            "access_token": self.sign(&access),
            "id_token": self.sign(&id_claims),
            "token_type": "Bearer",
            "expires_in": TOKEN_TTL_SECS,
            "scope": approval.scope,
        })
    }

    /// Checks `client_id` may send the browser back to `redirect_uri`: the SPA client
    /// anywhere, a registered client only to its own URIs, and a client ID metadata
    /// document's URL only to the URIs that document lists.
    async fn check_client(&self, client_id: &str, redirect_uri: &str) -> Result<(), String> {
        if client_id == self.client_id {
            return Ok(());
        }
        let registered = self
            .registered
            .lock()
            .expect("not poisoned")
            .get(client_id)
            .cloned();
        if let Some(uris) = registered {
            return if uris.iter().any(|uri| uri == redirect_uri) {
                Ok(())
            } else {
                Err(format!(
                    "`{redirect_uri}` isn't registered for `{client_id}`"
                ))
            };
        }
        let Some(url) = cimd_url(client_id) else {
            return Err(format!("unknown client_id `{client_id}`"));
        };
        let document = self.fetch_cimd(url).await?;
        if document["client_id"].as_str() != Some(client_id) {
            return Err("the metadata document's client_id isn't its URL".to_string());
        }
        let listed = document["redirect_uris"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|uri| uri.as_str() == Some(redirect_uri));
        if !listed {
            return Err(format!("`{redirect_uri}` isn't in the metadata document"));
        }
        Ok(())
    }

    async fn fetch_cimd(&self, url: Url) -> Result<Value, String> {
        let mut answer = self
            .http
            .get(url)
            .header("accept", "application/json")
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| format!("fetching the metadata document: {e}"))?;
        let mut body = Vec::new();
        while let Some(chunk) = answer
            .chunk()
            .await
            .map_err(|e| format!("reading the metadata document: {e}"))?
        {
            body.extend_from_slice(&chunk);
            if body.len() > CIMD_MAX_BYTES {
                return Err("the metadata document is too large".to_string());
            }
        }
        serde_json::from_slice(&body).map_err(|e| format!("the metadata document: {e}"))
    }

    /// Whether `header` is a bearer access token from this provider granting [`REGISTER_SCOPE`].
    fn may_register(&self, header: Option<&str>) -> bool {
        let Some(bearer) = header.and_then(|h| h.strip_prefix("Bearer ")) else {
            return false;
        };
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.issuer]);
        validation.validate_aud = false;
        decode::<Value>(bearer, &self.decoding, &validation).is_ok_and(|data| {
            data.claims["token_use"] == "access"
                && data.claims["scope"]
                    .as_str()
                    .is_some_and(|s| s.split(' ').any(|s| s == REGISTER_SCOPE))
        })
    }
}

/// A client ID that names a metadata document: an `https` URL, or `http` on
/// loopback for tests.
fn cimd_url(client_id: &str) -> Option<Url> {
    let url = Url::parse(client_id).ok()?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    (url.scheme() == "https" || (url.scheme() == "http" && loopback)).then_some(url)
}

/// Served as both OIDC discovery and RFC 8414 authorization server metadata.
#[handler]
fn discovery(provider: Data<&Arc<Provider>>) -> Json<Value> {
    let issuer = &provider.issuer;
    Json(json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "registration_endpoint": format!("{issuer}/oauth2/register"),
        "jwks_uri": format!("{issuer}/jwks"),
        "end_session_endpoint": format!("{issuer}/end-session"),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "code_challenge_methods_supported": ["S256"],
        "grant_types_supported": ["authorization_code"],
        "token_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": ["openid", "profile", "email", REGISTER_SCOPE],
        "client_id_metadata_document_supported": true,
    }))
}

#[handler]
fn jwks(provider: Data<&Arc<Provider>>) -> Json<JwkSet> {
    Json(provider.jwks.clone())
}

#[derive(Deserialize)]
struct AuthorizeRequest {
    client_id: String,
    redirect_uri: String,
    response_type: String,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    state: Option<String>,
    nonce: Option<String>,
    scope: Option<String>,
    resource: Option<String>,
}

/// Approves at once, as the one subject, with no page of its own.
#[handler]
async fn authorize(
    provider: Data<&Arc<Provider>>,
    Query(request): Query<AuthorizeRequest>,
) -> poem::Result<Redirect> {
    provider
        .check_client(&request.client_id, &request.redirect_uri)
        .await
        .map_err(refuse)?;
    if request.response_type != "code" {
        return Err(refuse("only response_type=code"));
    }
    let (Some(code_challenge), Some("S256")) = (
        request.code_challenge,
        request.code_challenge_method.as_deref(),
    ) else {
        return Err(refuse("PKCE with S256 is required"));
    };
    let mut back = Url::parse(&request.redirect_uri).map_err(refuse)?;
    let code = random_token();
    provider.codes.lock().expect("not poisoned").insert(
        code.clone(),
        Approval {
            client_id: request.client_id,
            redirect_uri: request.redirect_uri,
            code_challenge,
            nonce: request.nonce,
            scope: request
                .scope
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_SCOPE.to_string()),
            resource: request.resource,
        },
    );
    back.query_pairs_mut().append_pair("code", &code);
    if let Some(state) = &request.state {
        back.query_pairs_mut().append_pair("state", state);
    }
    Ok(Redirect::see_other(back.as_str()))
}

#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    code: String,
    redirect_uri: String,
    code_verifier: String,
    client_id: String,
}

#[handler]
fn token(provider: Data<&Arc<Provider>>, Form(request): Form<TokenRequest>) -> Response {
    if request.grant_type != "authorization_code" {
        return token_error("unsupported_grant_type");
    }
    let Some(approval) = provider
        .codes
        .lock()
        .expect("not poisoned")
        .remove(&request.code)
    else {
        return token_error("invalid_grant");
    };
    if approval.client_id != request.client_id {
        return token_error("invalid_client");
    }
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(request.code_verifier.as_bytes()));
    if approval.redirect_uri != request.redirect_uri || approval.code_challenge != challenge {
        return token_error("invalid_grant");
    }
    Json(provider.tokens(approval)).into_response()
}

/// RFC 7591 dynamic client registration at Cognito's path. Registers a public
/// client (`token_endpoint_auth_method: none`) and answers its metadata.
#[handler]
fn register(provider: Data<&Arc<Provider>>, req: &poem::Request, body: String) -> Response {
    if provider.registration_requires_token {
        let header = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok());
        if !provider.may_register(header) {
            return oauth_error(
                StatusCode::UNAUTHORIZED,
                "invalid_token",
                &format!("a bearer access token with scope `{REGISTER_SCOPE}` is required"),
            );
        }
    }
    let Ok(Value::Object(metadata)) = serde_json::from_str::<Value>(&body) else {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_client_metadata",
            "the body isn't a JSON object",
        );
    };
    let uris: Option<Vec<String>> = metadata
        .get("redirect_uris")
        .and_then(Value::as_array)
        .filter(|uris| !uris.is_empty())
        .and_then(|uris| {
            uris.iter()
                .map(|uri| {
                    uri.as_str()
                        .filter(|uri| Url::parse(uri).is_ok())
                        .map(str::to_owned)
                })
                .collect()
        });
    let Some(uris) = uris else {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            "redirect_uris must be a non-empty array of absolute URIs",
        );
    };
    let auth_method = metadata
        .get("token_endpoint_auth_method")
        .map_or(Some("none"), Value::as_str);
    if auth_method != Some("none") {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_client_metadata",
            "only token_endpoint_auth_method `none` is supported",
        );
    }
    let client_id = format!("dcr-{}", random_token());
    let mut answer = metadata.clone();
    answer.insert("client_id".into(), json!(client_id));
    answer.insert("client_id_issued_at".into(), json!(Provider::now()));
    answer.insert("redirect_uris".into(), json!(uris));
    answer.insert("token_endpoint_auth_method".into(), json!("none"));
    answer
        .entry("grant_types")
        .or_insert(json!(["authorization_code"]));
    answer.entry("response_types").or_insert(json!(["code"]));
    provider
        .registered
        .lock()
        .expect("not poisoned")
        .insert(client_id, uris);
    (StatusCode::CREATED, Json(Value::Object(answer))).into_response()
}

fn oauth_error(status: StatusCode, error: &str, description: &str) -> Response {
    (
        status,
        Json(json!({ "error": error, "error_description": description })),
    )
        .into_response()
}

#[derive(Deserialize)]
struct EndSessionRequest {
    post_logout_redirect_uri: Option<String>,
    state: Option<String>,
}

#[handler]
fn end_session(Query(request): Query<EndSessionRequest>) -> poem::Result<Response> {
    let Some(uri) = request.post_logout_redirect_uri else {
        return Ok("signed out".into_response());
    };
    let mut back = Url::parse(&uri).map_err(refuse)?;
    if let Some(state) = &request.state {
        back.query_pairs_mut().append_pair("state", state);
    }
    Ok(Redirect::see_other(back.as_str()).into_response())
}

#[derive(Serialize)]
struct TokenError {
    error: &'static str,
}

fn token_error(error: &'static str) -> Response {
    (StatusCode::BAD_REQUEST, Json(TokenError { error })).into_response()
}

fn refuse(message: impl ToString) -> poem::Error {
    poem::Error::from_string(message.to_string(), StatusCode::BAD_REQUEST)
}

fn random_token() -> String {
    let bytes: [u8; 16] = thread_rng().r#gen();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// A stand-in for Cognito's user pool API (AWS JSON 1.1, `X-Amz-Target:
/// AWSCognitoIdentityProviderService.<Operation>`), for the admin operations `access`
/// invites through: `AdminCreateUser`, `AdminGetUser`, `AdminDeleteUser` and
/// `DescribeUserPool`. It checks no signature. Users it creates are kept by email,
/// each given a fresh `sub`, in `FORCE_CHANGE_PASSWORD`; served at `/cognito`.
#[derive(Default)]
pub struct UserPool {
    accounts: Mutex<HashMap<String, PoolUser>>,
}

#[derive(Clone)]
struct PoolUser {
    sub: String,
    email: String,
    name: Option<String>,
}

impl UserPool {
    pub fn endpoint(self: Arc<Self>) -> impl Endpoint {
        Route::new().at("/", post(cognito)).data(self)
    }
}

fn cognito_error(kind: &str, message: &str) -> Response {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header("content-type", "application/x-amz-json-1.1")
        .header("x-amzn-errortype", kind)
        .body(json!({ "__type": kind, "message": message }).to_string())
}

fn cognito_ok(body: Value) -> Response {
    Response::builder()
        .header("content-type", "application/x-amz-json-1.1")
        .body(body.to_string())
}

fn pool_user_json(user: &PoolUser, attributes_key: &str) -> Value {
    let mut attributes = vec![
        json!({ "Name": "sub", "Value": user.sub }),
        json!({ "Name": "email", "Value": user.email }),
        json!({ "Name": "email_verified", "Value": "true" }),
    ];
    if let Some(name) = &user.name {
        attributes.push(json!({ "Name": "name", "Value": name }));
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("after 1970")
        .as_secs();
    json!({
        "Username": user.email,
        attributes_key: attributes,
        "UserStatus": "FORCE_CHANGE_PASSWORD",
        "Enabled": true,
        "UserCreateDate": now,
        "UserLastModifiedDate": now,
    })
}

#[handler]
fn cognito(pool: Data<&Arc<UserPool>>, req: &poem::Request, body: String) -> Response {
    let operation = req
        .headers()
        .get("x-amz-target")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit('.').next())
        .unwrap_or_default()
        .to_owned();
    let Ok(input) = serde_json::from_str::<Value>(&body) else {
        return cognito_error("InvalidParameterException", "the body isn't JSON");
    };
    let username = input["Username"].as_str().unwrap_or_default().to_owned();
    let mut accounts = pool.accounts.lock().expect("not poisoned");
    match operation.as_str() {
        "DescribeUserPool" => cognito_ok(json!({ "UserPool": {
            "Id": input["UserPoolId"],
            "Policies": { "PasswordPolicy": { "TemporaryPasswordValidityDays": 7 } },
        }})),
        "AdminCreateUser" => {
            let resend = input["MessageAction"].as_str() == Some("RESEND");
            match (accounts.get(&username), resend) {
                (Some(user), true) => {
                    cognito_ok(json!({ "User": pool_user_json(user, "Attributes") }))
                }
                (None, true) => cognito_error("UserNotFoundException", "User does not exist."),
                (Some(_), false) => {
                    cognito_error("UsernameExistsException", "User account already exists.")
                }
                (None, false) => {
                    let attribute = |name: &str| {
                        input["UserAttributes"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|a| a["Name"] == name)
                            .and_then(|a| a["Value"].as_str())
                            .map(str::to_owned)
                    };
                    let user = PoolUser {
                        sub: format!("invited-{}", random_token()),
                        email: attribute("email").unwrap_or_else(|| username.clone()),
                        name: attribute("name"),
                    };
                    let answer = cognito_ok(json!({ "User": pool_user_json(&user, "Attributes") }));
                    accounts.insert(username, user);
                    answer
                }
            }
        }
        "AdminGetUser" => match accounts.get(&username) {
            Some(user) => cognito_ok(pool_user_json(user, "UserAttributes")),
            None => cognito_error("UserNotFoundException", "User does not exist."),
        },
        "AdminDeleteUser" => match accounts.remove(&username) {
            Some(_) => cognito_ok(json!({})),
            None => cognito_error("UserNotFoundException", "User does not exist."),
        },
        other => cognito_error(
            "UnknownOperationException",
            &format!("`{other}` isn't faked"),
        ),
    }
}

impl UserPool {
    /// A fresh pool served on 127.0.0.1 on a thread of its own, with its own runtime, so it
    /// outlives any one test's: its endpoint URL, for `COGNITO_ENDPOINT`.
    pub fn serve() -> String {
        let (bound, addr) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .expect("a runtime")
                .block_on(async move {
                    use poem::listener::{Acceptor, Listener, TcpListener};
                    let acceptor = TcpListener::bind("127.0.0.1:0")
                        .into_acceptor()
                        .await
                        .expect("binds");
                    let addr = acceptor
                        .local_addr()
                        .into_iter()
                        .find_map(|addr| addr.as_socket_addr().copied())
                        .expect("a socket address");
                    bound.send(addr).expect("the caller waits for it");
                    poem::Server::new_with_acceptor(acceptor)
                        .run(Arc::new(UserPool::default()).endpoint())
                        .await
                })
        });
        format!("http://{}", addr.recv().expect("the user pool binds"))
    }
}
