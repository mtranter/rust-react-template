use serde::{Deserialize, Serialize};

/// A Connection a module needs (ADR 0050): code declares its shape, and an admin of the running
/// app fills in its URL, fields and secrets. A module registers it with
/// `Module::declaring(Declaration::new(..))`, and `migrate` records it.
///
/// ```
/// use connections_api::{Auth, Declaration, HealthCheck};
///
/// let crm = Declaration::new("crm", "CRM")
///     .auth(Auth::ApiKeyHeader { name: "X-Api-Key".into() })
///     .field("tenant", "Tenant")
///     .health_check(HealthCheck::get("/health"));
/// assert_eq!(crm.secrets(), [("api_key", "API key")]);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Declaration {
    /// Unique across the app: lowercase letters, digits, `-` and `_`, from a letter.
    pub key: String,
    pub label: String,
    pub auth: Auth,
    /// The non-secret values the admin supplies beside the URL, besides the auth style's own.
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub health_check: Option<HealthCheck>,
}

impl Declaration {
    /// With no auth, no fields and no health check.
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            auth: Auth::None,
            fields: Vec::new(),
            health_check: None,
        }
    }

    pub fn auth(self, auth: Auth) -> Self {
        Self { auth, ..self }
    }

    pub fn field(mut self, name: impl Into<String>, label: impl Into<String>) -> Self {
        self.fields.push(Field {
            name: name.into(),
            label: label.into(),
        });
        self
    }

    pub fn health_check(self, health_check: HealthCheck) -> Self {
        Self {
            health_check: Some(health_check),
            ..self
        }
    }

    /// Every non-secret field, the auth style's first, as `(name, label)`.
    pub fn all_fields(&self) -> Vec<(&str, &str)> {
        self.auth
            .fields()
            .iter()
            .copied()
            .chain(
                self.fields
                    .iter()
                    .map(|f| (f.name.as_str(), f.label.as_str())),
            )
            .collect()
    }

    /// The secrets its auth style takes, as `(name, label)`.
    pub fn secrets(&self) -> &'static [(&'static str, &'static str)] {
        self.auth.secrets()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    pub label: String,
}

/// How a request to the other system authenticates. User-delegated OAuth is left out (ADR 0050).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "style", rename_all = "snake_case")]
pub enum Auth {
    None,
    /// `Authorization: Bearer <token>`.
    Bearer,
    /// `Authorization: Basic`, of the `username` field and the `password` secret.
    Basic,
    /// The `api_key` secret in the header `name`.
    ApiKeyHeader {
        name: String,
    },
    /// The `api_key` secret in the query parameter `name`.
    ApiKeyQuery {
        name: String,
    },
    /// A token from the `token_url` field's endpoint for the `client_id` field and the
    /// `client_secret` secret, asking for `scopes`.
    #[serde(rename = "oauth2_client_credentials")]
    OAuth2ClientCredentials {
        #[serde(default)]
        scopes: Vec<String>,
    },
}

impl Auth {
    /// The non-secret fields the style takes, as `(name, label)`.
    pub fn fields(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            Auth::Basic => &[("username", "Username")],
            Auth::OAuth2ClientCredentials { .. } => {
                &[("token_url", "Token URL"), ("client_id", "Client ID")]
            }
            Auth::None | Auth::Bearer | Auth::ApiKeyHeader { .. } | Auth::ApiKeyQuery { .. } => &[],
        }
    }

    /// The secrets the style takes, as `(name, label)`.
    pub fn secrets(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            Auth::None => &[],
            Auth::Bearer => &[("token", "Token")],
            Auth::Basic => &[("password", "Password")],
            Auth::ApiKeyHeader { .. } | Auth::ApiKeyQuery { .. } => &[("api_key", "API key")],
            Auth::OAuth2ClientCredentials { .. } => &[("client_secret", "Client secret")],
        }
    }
}

/// A request that answers 2xx when the Connection works, relative to its URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub method: Method,
    pub path: String,
}

impl HealthCheck {
    pub fn get(path: impl Into<String>) -> Self {
        Self {
            method: Method::Get,
            path: path.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Head,
    Post,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declaration_round_trips_as_stored() {
        for auth in [
            Auth::None,
            Auth::Bearer,
            Auth::Basic,
            Auth::ApiKeyHeader {
                name: "X-Api-Key".into(),
            },
            Auth::ApiKeyQuery { name: "key".into() },
            Auth::OAuth2ClientCredentials {
                scopes: vec!["read".into()],
            },
        ] {
            let declaration = Declaration::new("crm", "CRM")
                .auth(auth)
                .field("tenant", "Tenant")
                .health_check(HealthCheck::get("/health"));
            let stored = serde_json::to_value(&declaration).unwrap();
            assert_eq!(
                serde_json::from_value::<Declaration>(stored).unwrap(),
                declaration
            );
        }
    }
}
