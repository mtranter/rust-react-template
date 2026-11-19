//! An OIDC provider for end-to-end checks: discovery, JWKS, an authorize
//! endpoint that approves straight away as one named subject, a token endpoint
//! for the authorization code with PKCE, and end-session. Its RS256 key is made
//! at start, so its tokens verify only against the JWKS it serves.

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
