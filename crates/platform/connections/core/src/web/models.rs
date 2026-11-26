//! Timestamps and the version are plain strings so the generated TS client stays
//! dependency-free. No response carries a secret's value, sealed or not.

use crate::application::{ConnectionSortKey, Versioned};
use crate::domain::Connection;
use app_persistence::{Direction, Sort};
use app_web::SortParam;
use connections_api::{Auth, Method};
use poem_openapi::types::MaybeUndefined;
use poem_openapi::{Enum, Object};
use std::collections::BTreeMap;

/// Carries no version: it travels only in the `If-Match` header.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct UpdateConnectionRequest {
    /// The base URL requests are sent to, absolute `http` or `https`: left as it is when absent,
    /// cleared by `null` or an empty string.
    pub url: MaybeUndefined<String>,
    /// Declared fields to set, by name; an empty value clears one. Fields not named are left
    /// as they are.
    pub fields: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct SetSecretRequest {
    /// Stored sealed, and never answered back.
    #[oai(write_only)]
    pub value: String,
}

#[derive(Debug, Clone, Copy, Enum)]
#[oai(rename_all = "snake_case")]
pub enum AuthStyle {
    None,
    Bearer,
    Basic,
    ApiKeyHeader,
    ApiKeyQuery,
    #[oai(rename = "oauth2_client_credentials")]
    OAuth2ClientCredentials,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct AuthResponse {
    pub style: AuthStyle,
    /// The header or query parameter an API key is sent in.
    pub name: Option<String>,
    /// What an OAuth 2 client-credentials token is asked for.
    pub scopes: Vec<String>,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct FieldResponse {
    pub name: String,
    pub label: String,
    /// `None` until an admin sets it.
    pub value: Option<String>,
}

/// Whether a secret is set, and when and by whom: never its value.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct SecretResponse {
    pub name: String,
    pub label: String,
    pub set: bool,
    pub set_at: Option<String>,
    /// The subject who last set it.
    pub set_by: Option<String>,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct HealthCheckResponse {
    /// `GET`, `HEAD` or `POST`.
    pub method: String,
    /// Relative to the Connection's URL.
    pub path: String,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct ConnectionResponse {
    /// As the module that needs it declares it.
    pub key: String,
    pub label: String,
    pub auth: AuthResponse,
    pub url: Option<String>,
    /// The auth style's fields first, then the module's.
    pub fields: Vec<FieldResponse>,
    /// The secrets its auth style takes.
    pub secrets: Vec<SecretResponse>,
    pub health_check: Option<HealthCheckResponse>,
    /// Optimistic-concurrency version. Echo it back in `If-Match` to change this Connection or
    /// its secrets; a stale value is refused with `412 Precondition Failed`. Also mirrored in
    /// the `ETag` response header for single-item reads.
    pub version: String,
}

impl From<&Auth> for AuthResponse {
    fn from(auth: &Auth) -> Self {
        let (style, name, scopes) = match auth {
            Auth::None => (AuthStyle::None, None, Vec::new()),
            Auth::Bearer => (AuthStyle::Bearer, None, Vec::new()),
            Auth::Basic => (AuthStyle::Basic, None, Vec::new()),
            Auth::ApiKeyHeader { name } => {
                (AuthStyle::ApiKeyHeader, Some(name.clone()), Vec::new())
            }
            Auth::ApiKeyQuery { name } => (AuthStyle::ApiKeyQuery, Some(name.clone()), Vec::new()),
            Auth::OAuth2ClientCredentials { scopes } => {
                (AuthStyle::OAuth2ClientCredentials, None, scopes.clone())
            }
        };
        Self {
            style,
            name,
            scopes,
        }
    }
}

impl From<Versioned<Connection>> for ConnectionResponse {
    fn from(versioned: Versioned<Connection>) -> Self {
        let Versioned { entity: c, version } = versioned;
        let declaration = c.declaration();
        Self {
            key: c.key().to_string(),
            label: c.label().to_owned(),
            auth: c.auth().into(),
            url: c.url().map(str::to_owned),
            fields: declaration
                .all_fields()
                .into_iter()
                .map(|(name, label)| FieldResponse {
                    name: name.to_owned(),
                    label: label.to_owned(),
                    value: c.fields().get(name).cloned(),
                })
                .collect(),
            secrets: declaration
                .secrets()
                .iter()
                .map(|(name, label)| {
                    let secret = c.secret(name);
                    SecretResponse {
                        name: (*name).to_owned(),
                        label: (*label).to_owned(),
                        set: secret.is_some(),
                        set_at: secret.map(|s| s.set_at.to_rfc3339()),
                        set_by: secret.map(|s| s.set_by.clone()),
                    }
                })
                .collect(),
            health_check: declaration
                .health_check
                .as_ref()
                .map(|check| HealthCheckResponse {
                    method: match check.method {
                        Method::Get => "GET",
                        Method::Head => "HEAD",
                        Method::Post => "POST",
                    }
                    .to_owned(),
                    path: check.path.clone(),
                }),
            version: version.as_i64().to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Enum)]
pub enum ConnectionSort {
    #[oai(rename = "key")]
    KeyAsc,
    #[oai(rename = "-key")]
    KeyDesc,
    #[oai(rename = "label")]
    LabelAsc,
    #[oai(rename = "-label")]
    LabelDesc,
}

impl SortParam for ConnectionSort {
    type Key = ConnectionSortKey;

    fn sort(self) -> Sort<ConnectionSortKey> {
        let (key, direction) = match self {
            Self::KeyAsc => (ConnectionSortKey::Key, Direction::Asc),
            Self::KeyDesc => (ConnectionSortKey::Key, Direction::Desc),
            Self::LabelAsc => (ConnectionSortKey::Label, Direction::Asc),
            Self::LabelDesc => (ConnectionSortKey::Label, Direction::Desc),
        };
        Sort { key, direction }
    }
}
