//! An OIDC provider for end-to-end checks: discovery, JWKS, an authorize
//! endpoint that approves straight away as one named subject, a token endpoint
//! for the authorization code with PKCE, and end-session. Its RS256 key is made
//! at start, so its tokens verify only against the JWKS it serves. Beside it, a
//! stand-in for the Cognito user pool API invitations go through ([`UserPool`]).

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::jwk::{Jwk, JwkSet, PublicKeyUse};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
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
    jwks: JwkSet,
    codes: Mutex<HashMap<String, Approval>>,
}

/// What an authorization code was issued for, checked when it is redeemed.
struct Approval {
    redirect_uri: String,
    code_challenge: String,
    nonce: Option<String>,
}

impl Provider {
    /// `issuer` is where it is reached, without a trailing `/`: tokens carry it as `iss`.
    pub fn new(issuer: impl Into<String>, client_id: impl Into<String>, subject: Subject) -> Self {
        let der = RsaPrivateKey::new(&mut thread_rng(), 2048)
            .and_then(|key| Ok(key.to_pkcs1_der()?))
            .expect("an RSA key");
        let key = EncodingKey::from_rsa_der(der.as_bytes());
        let mut jwk = Jwk::from_encoding_key(&key, Algorithm::RS256).expect("the key's JWK");
        jwk.common.key_id = Some(KEY_ID.to_string());
        jwk.common.public_key_use = Some(PublicKeyUse::Signature);
        Self {
            issuer: issuer.into(),
            client_id: client_id.into(),
            subject,
            key,
            jwks: JwkSet { keys: vec![jwk] },
            codes: Mutex::new(HashMap::new()),
        }
    }

    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// The provider's routes, open to any origin: the SPA's browser calls them.
    pub fn endpoint(self: Arc<Self>) -> impl Endpoint {
        Route::new()
            .at("/.well-known/openid-configuration", get(discovery))
            .at("/jwks", get(jwks))
            .at("/authorize", get(authorize))
            .at("/token", post(token))
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

    fn tokens(&self, nonce: Option<String>) -> Value {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("after 1970")
            .as_secs();
        let Subject { sub, name, email } = &self.subject;
        let claims = json!({
            "iss": self.issuer,
            "sub": sub,
            "aud": self.client_id,
            "iat": now,
            "exp": now + TOKEN_TTL_SECS,
            "name": name,
            "email": email,
        });
        let mut id_claims = claims.clone();
        if let Some(nonce) = nonce {
            id_claims["nonce"] = json!(nonce);
        }
        json!({
            "access_token": self.sign(&claims),
            "id_token": self.sign(&id_claims),
            "token_type": "Bearer",
            "expires_in": TOKEN_TTL_SECS,
            "scope": "openid profile email",
        })
    }
}

#[handler]
fn discovery(provider: Data<&Arc<Provider>>) -> Json<Value> {
    let issuer = &provider.issuer;
    Json(json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "jwks_uri": format!("{issuer}/jwks"),
        "end_session_endpoint": format!("{issuer}/end-session"),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "code_challenge_methods_supported": ["S256"],
        "grant_types_supported": ["authorization_code"],
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
}

/// Approves at once, as the one subject, with no page of its own.
#[handler]
fn authorize(
    provider: Data<&Arc<Provider>>,
    Query(request): Query<AuthorizeRequest>,
) -> poem::Result<Redirect> {
    if request.client_id != provider.client_id {
        return Err(refuse(format!("unknown client_id `{}`", request.client_id)));
    }
    if request.response_type != "code" {
        return Err(refuse("only response_type=code"));
    }
    let (Some(code_challenge), Some("S256")) = (
        request.code_challenge,
        request.code_challenge_method.as_deref(),
    ) else {
        return Err(refuse("PKCE with S256 is required"));
    };
    let code = random_token();
    provider.codes.lock().expect("not poisoned").insert(
        code.clone(),
        Approval {
            redirect_uri: request.redirect_uri.clone(),
            code_challenge,
            nonce: request.nonce,
        },
    );
    let mut back = Url::parse(&request.redirect_uri).map_err(refuse)?;
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
    if request.client_id != provider.client_id {
        return token_error("invalid_client");
    }
    let Some(approval) = provider
        .codes
        .lock()
        .expect("not poisoned")
        .remove(&request.code)
    else {
        return token_error("invalid_grant");
    };
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(request.code_verifier.as_bytes()));
    if approval.redirect_uri != request.redirect_uri || approval.code_challenge != challenge {
        return token_error("invalid_grant");
    }
    Json(provider.tokens(approval.nonce)).into_response()
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
