//! Calling out through a Connection (ADR 0050): the client a module asks for with
//! `client("crm")`, carrying the Connection's URL, auth and timeouts, spanned with nothing of its
//! secrets.

use super::{KeyProvider, open, sealed_for};
use crate::domain::Connection;
use app_domain::Clock;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, TimeDelta, Utc};
use connections_api::{Auth, CallError, Client, HttpMethod, Request, Response};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use tracing::Instrument;

/// A request as it goes on the wire: its URL absolute, its auth applied.
pub struct Outbound {
    pub method: HttpMethod,
    pub url: url::Url,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// Sends a request to another system: the one place HTTP is spoken. Its errors must not
/// carry the URL, whose query may hold an API key.
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, request: Outbound) -> Result<Response, CallError>;
}

/// A cached OAuth 2 token, for the Connection's version it was got for: any change to the
/// Connection (its secret rotated, its token URL changed) asks for a new one.
struct Token {
    version: i64,
    value: String,
    expires_at: DateTime<Utc>,
}

/// Every Connection's cached token, by key.
#[derive(Default)]
pub struct Tokens(Mutex<HashMap<String, Token>>);

impl Tokens {
    fn get(&self, key: &str, version: i64, now: DateTime<Utc>) -> Option<String> {
        let tokens = self.0.lock().expect("the token cache isn't poisoned");
        tokens
            .get(key)
            .filter(|t| t.version == version && t.expires_at > now)
            .map(|t| t.value.clone())
    }

    fn put(&self, key: &str, token: Token) {
        let mut tokens = self.0.lock().expect("the token cache isn't poisoned");
        tokens.insert(key.to_owned(), token);
    }
}

/// A token is renewed this long before it expires, so none expires in flight.
const TOKEN_LEEWAY: TimeDelta = TimeDelta::seconds(30);
/// When the token endpoint doesn't say (`expires_in` is only RECOMMENDED, RFC 6749 §5.1).
const DEFAULT_TOKEN_LIFETIME: TimeDelta = TimeDelta::hours(1);

/// The opened values a Connection's auth needs. Never `Debug`.
enum Credentials {
    None,
    Bearer(String),
    Basic(String),
    ApiKeyHeader {
        name: String,
        key: String,
    },
    ApiKeyQuery {
        name: String,
        key: String,
    },
    OAuth2 {
        token_url: url::Url,
        client_id: String,
        client_secret: String,
        scopes: Vec<String>,
    },
}

pub struct ConnectionClient {
    key: String,
    version: i64,
    base: url::Url,
    credentials: Credentials,
    transport: Arc<dyn Transport>,
    tokens: Arc<Tokens>,
    clock: Arc<dyn Clock>,
}

impl ConnectionClient {
    /// `None` keys can't open the secrets: as unconfigured as a secret not set.
    pub async fn open(
        connection: &Connection,
        version: i64,
        keys: Option<&dyn KeyProvider>,
        transport: Arc<dyn Transport>,
        tokens: Arc<Tokens>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, CallError> {
        let missing = connection.missing();
        if !missing.is_empty() {
            return Err(CallError::NotConfigured(format!(
                "the Connection `{}` lacks its {}",
                connection.key(),
                missing.join(", ")
            )));
        }
        let base = url::Url::parse(connection.url().unwrap_or_default())
            .map_err(|_| not_configured(connection, "its URL isn't valid"))?;
        let fields = connection.fields();
        let field = |name: &str| fields.get(name).cloned().unwrap_or_default();
        let mut secrets = BTreeMap::new();
        for name in connection.secrets().keys() {
            let keys = keys.ok_or_else(|| {
                not_configured(
                    connection,
                    "no master key is configured to open its secrets",
                )
            })?;
            let secret = connection.secret(name).expect("listed by name");
            let value = open(keys, &sealed_for(connection.key(), name), &secret.sealed)
                .await
                .map_err(|_| {
                    not_configured(
                        connection,
                        "its secrets don't open with this master key: set them again",
                    )
                })?;
            secrets.insert(name.as_str(), value);
        }
        let mut secret = |name: &str| secrets.remove(name).unwrap_or_default();
        let credentials = match connection.auth() {
            Auth::None => Credentials::None,
            Auth::Bearer => Credentials::Bearer(secret("token")),
            Auth::Basic => Credentials::Basic(STANDARD.encode(format!(
                "{}:{}",
                field("username"),
                secret("password")
            ))),
            Auth::ApiKeyHeader { name } => Credentials::ApiKeyHeader {
                name: name.clone(),
                key: secret("api_key"),
            },
            Auth::ApiKeyQuery { name } => Credentials::ApiKeyQuery {
                name: name.clone(),
                key: secret("api_key"),
            },
            Auth::OAuth2ClientCredentials { scopes } => Credentials::OAuth2 {
                token_url: url::Url::parse(&field("token_url"))
                    .map_err(|_| not_configured(connection, "its token URL isn't a URL"))?,
                client_id: field("client_id"),
                client_secret: secret("client_secret"),
                scopes: scopes.clone(),
            },
        };
        Ok(Self {
            key: connection.key().to_string(),
            version,
            base,
            credentials,
            transport,
            tokens,
            clock,
        })
    }

    fn url(&self, request: &Request) -> url::Url {
        let mut url = self.base.clone();
        let base_path = url.path().trim_end_matches('/').to_owned();
        let path = request.path.trim_start_matches('/');
        url.set_path(&format!("{base_path}/{path}"));
        {
            let mut query = url.query_pairs_mut();
            for (name, value) in &request.query {
                query.append_pair(name, value);
            }
        }
        if url.query() == Some("") {
            url.set_query(None);
        }
        url
    }

    async fn token(&self) -> Result<Option<String>, CallError> {
        let Credentials::OAuth2 {
            token_url,
            client_id,
            client_secret,
            scopes,
        } = &self.credentials
        else {
            return Ok(None);
        };
        let now = self.clock.now();
        if let Some(token) = self.tokens.get(&self.key, self.version, now) {
            return Ok(Some(token));
        }
        let form = {
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            form.append_pair("grant_type", "client_credentials");
            if !scopes.is_empty() {
                form.append_pair("scope", &scopes.join(" "));
            }
            form.finish()
        };
        // RFC 6749 §2.3.1: the client authenticates with HTTP Basic, each part form-encoded.
        let basic = STANDARD.encode(format!(
            "{}:{}",
            form_encoded(client_id),
            form_encoded(client_secret)
        ));
        let span = tracing::info_span!(
            "connection_token",
            otel.kind = "client",
            connection = %self.key,
            http.response.status_code = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        let answer = self
            .transport
            .send(Outbound {
                method: HttpMethod::Post,
                url: token_url.clone(),
                headers: vec![
                    ("Authorization".into(), format!("Basic {basic}")),
                    (
                        "Content-Type".into(),
                        "application/x-www-form-urlencoded".into(),
                    ),
                    ("Accept".into(), "application/json".into()),
                ],
                body: Some(form.into_bytes()),
            })
            .instrument(span.clone())
            .await
            .inspect_err(|_| {
                span.record("otel.status_code", "error");
            })?;
        span.record("http.response.status_code", answer.status);
        if !answer.is_success() {
            span.record("otel.status_code", "error");
            return Err(CallError::Unauthorized(format!(
                "the token endpoint refused the client credentials ({})",
                status_text(answer.status)
            )));
        }
        #[derive(serde::Deserialize)]
        struct Granted {
            access_token: String,
            expires_in: Option<i64>,
        }
        let granted: Granted = answer.json().map_err(|_| {
            CallError::Unreadable("the token endpoint's answer holds no access token".into())
        })?;
        let lifetime = granted
            .expires_in
            .map(TimeDelta::seconds)
            .unwrap_or(DEFAULT_TOKEN_LIFETIME);
        self.tokens.put(
            &self.key,
            Token {
                version: self.version,
                value: granted.access_token.clone(),
                expires_at: now + lifetime - TOKEN_LEEWAY,
            },
        );
        Ok(Some(granted.access_token))
    }
}

#[async_trait::async_trait]
impl Client for ConnectionClient {
    async fn send(&self, request: Request) -> Result<Response, CallError> {
        // The path, never the URL: an API key may ride in its query.
        let span = tracing::info_span!(
            "connection_call",
            otel.name = format!("{} {}", request.method.as_str(), self.key),
            otel.kind = "client",
            connection = %self.key,
            http.request.method = request.method.as_str(),
            url.path = %request.path,
            http.response.status_code = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        async {
            let mut url = self.url(&request);
            let mut headers = request.headers;
            match &self.credentials {
                Credentials::None => {}
                Credentials::Bearer(token) => {
                    headers.push(("Authorization".into(), format!("Bearer {token}")))
                }
                Credentials::Basic(encoded) => {
                    headers.push(("Authorization".into(), format!("Basic {encoded}")))
                }
                Credentials::ApiKeyHeader { name, key } => {
                    headers.push((name.clone(), key.clone()))
                }
                Credentials::ApiKeyQuery { name, key } => {
                    url.query_pairs_mut().append_pair(name, key);
                }
                Credentials::OAuth2 { .. } => {
                    let token = self.token().await?.unwrap_or_default();
                    headers.push(("Authorization".into(), format!("Bearer {token}")));
                }
            }
            let answer = self
                .transport
                .send(Outbound {
                    method: request.method,
                    url,
                    headers,
                    body: request.body,
                })
                .await?;
            let span = tracing::Span::current();
            span.record("http.response.status_code", answer.status);
            if answer.status >= 500 {
                span.record("otel.status_code", "error");
            }
            Ok(answer)
        }
        .instrument(span.clone())
        .await
        .inspect_err(|_| {
            span.record("otel.status_code", "error");
        })
    }
}

fn not_configured(connection: &Connection, why: &str) -> CallError {
    CallError::NotConfigured(format!("the Connection `{}`: {why}", connection.key()))
}

fn form_encoded(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// `401 Unauthorized`, as a person reads a status.
pub fn status_text(status: u16) -> String {
    let reason = match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    };
    format!("{status} {reason}").trim_end().to_owned()
}
