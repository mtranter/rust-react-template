//! Layering (later wins): defaults → optional `config.toml` → `APP_*` env vars →
//! `DATABASE_URL` (its conventional name, not `APP_DATABASE_URL`).

use crate::sign_in::{BASE_SCOPES, SignIn};
use app_bus::{Bus, NoBus, RedisBus};
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Dev,
    Test,
    Prod,
}

/// Taken raw, not through `Env`, which parses each value: an all-digit subject
/// or client id (as Google issues) would be a number, and `true` a bool.
const RAW: [&str; 4] = [
    "bootstrap_admins",
    "oidc_client_id",
    "oidc_scopes",
    "oidc_extra_params",
];

/// [`AppConfig::validate`] refuses to start in prod while this is in effect.
const DEV_JWT_SECRET: &str = "dev-only-insecure-secret-change-me";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub env: Environment,
    pub database_url: String,
    pub db_max_connections: u32,
    pub cors_origin: String,
    /// Ignored when `jwt_issuer` is set.
    pub jwt_secret: String,
    /// OIDC issuer; when set, tokens are verified via its JWKS instead of the secret.
    pub jwt_issuer: Option<String>,
    /// Unset disables audience checking.
    pub jwt_audience: Option<String>,
    /// Where clients reach the server, which the URLs it advertises to MCP
    /// clients start with. Unset, each request's `Host` and `X-Forwarded-*`
    /// headers say.
    pub public_url: Option<String>,
    /// How long the `worker` bin waits between polls once nothing is due.
    pub worker_poll_ms: u64,
    /// How often, in seconds, each `worker` compacts the outbox (ADR 0033),
    /// each on its own schedule.
    pub outbox_compact_every: u64,
    /// The Redis or Valkey that server instances and workers broadcast to
    /// each other over (ADR 0039): `rediss://user:password@host:6379` for
    /// TLS, as ElastiCache Serverless for Valkey requires. Required once any
    /// module broadcasts.
    pub redis_url: Option<String>,
    /// The CA certificate (PEM) a `rediss://` Redis's certificate is checked
    /// against, in place of the public roots, for one signed by a private
    /// CA. Unset (or empty), the public roots, which ElastiCache's are under.
    pub redis_ca: Option<String>,
    /// Subjects (the token's `sub`), comma-separated, that always hold `admin`
    /// whatever `access` assigns: how a fresh app gets its first admin.
    pub bootstrap_admins: String,
    /// The embedded SPA's public client id at the `jwt_issuer`.
    pub oidc_client_id: Option<String>,
    /// Scopes the SPA requests beyond `openid profile email`, space-separated
    /// as in OAuth's `scope` (`offline_access api://app/.default`).
    pub oidc_scopes: String,
    /// Parameters the SPA adds to its authorization request, space-separated
    /// `key=value` pairs (`audience=https://api.example.com`).
    pub oidc_extra_params: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            env: Environment::Dev,
            database_url: "postgres://app:app@localhost:5432/app".to_string(),
            db_max_connections: 10,
            cors_origin: "http://localhost:5173".to_string(),
            jwt_secret: DEV_JWT_SECRET.to_string(),
            jwt_issuer: None,
            jwt_audience: None,
            public_url: None,
            worker_poll_ms: 1000,
            outbox_compact_every: app_db::Worker::COMPACT_EVERY.num_seconds() as u64,
            redis_url: None,
            redis_ca: None,
            bootstrap_admins: String::new(),
            oidc_client_id: None,
            oidc_scopes: String::new(),
            oidc_extra_params: String::new(),
        }
    }
}

impl AppConfig {
    /// The error is boxed: `figment::Error` is large, and this is a cold path.
    pub fn load() -> Result<Self, Box<figment::Error>> {
        let raw = RAW.into_iter().filter_map(|key| {
            std::env::var(format!("APP_{}", key.to_ascii_uppercase()))
                .ok()
                .map(|value| (key, value))
        });
        let config: Self = Self::figment(raw).extract().map_err(Box::new)?;
        config.validate()?;
        Ok(config)
    }

    fn figment<'a>(raw: impl IntoIterator<Item = (&'a str, String)>) -> Figment {
        let figment = Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::file("config.toml"))
            .merge(Env::prefixed("APP_").ignore(&RAW))
            .merge(
                Env::raw()
                    .only(&["DATABASE_URL"])
                    .map(|key| key.as_str().to_ascii_lowercase().into()),
            );
        raw.into_iter().fold(figment, |figment, (key, value)| {
            figment.merge(Serialized::default(key, value))
        })
    }

    pub fn validate(&self) -> Result<(), Box<figment::Error>> {
        self.validate_serving(cfg!(feature = "embed-frontend"))
    }

    fn validate_serving(&self, spa_embedded: bool) -> Result<(), Box<figment::Error>> {
        let refuse = |message: &str| Err(Box::new(figment::Error::from(message.to_string())));
        // Without this, a copy-paste prod deploy would accept tokens anyone can forge.
        if self.is_prod() && self.jwt_issuer.is_none() && self.jwt_secret == DEV_JWT_SECRET {
            return refuse(
                "refusing to start in prod with the built-in dev JWT secret: \
                 set APP_JWT_SECRET, or configure APP_JWT_ISSUER for JWKS verification",
            );
        }
        // Without this, the SPA could sign no one in, and every guarded screen would be a 401.
        if self.is_prod() && spa_embedded && self.jwt_issuer.is_some() && self.client_id().is_none()
        {
            return refuse(
                "refusing to start in prod with APP_JWT_ISSUER set but APP_OIDC_CLIENT_ID unset: \
                 the embedded SPA signs in at the issuer as that client",
            );
        }
        self.extra_params().map(|_| ()).map_err(|message| {
            Box::new(figment::Error::from(format!(
                "refusing to start: APP_OIDC_EXTRA_PARAMS {message}"
            )))
        })
    }

    /// The SPA's sign-in settings, which `/auth/config` serves: `None` without
    /// an issuer, when there is nothing to sign in with.
    pub fn sign_in(&self) -> Option<SignIn> {
        let issuer = self.jwt_issuer.clone()?;
        let mut scopes: Vec<String> = BASE_SCOPES.map(str::to_owned).to_vec();
        for scope in self.oidc_scopes.split_whitespace() {
            if !scopes.iter().any(|s| s == scope) {
                scopes.push(scope.to_owned());
            }
        }
        Some(SignIn {
            issuer,
            client_id: self.client_id(),
            scopes,
            extra_params: self.extra_params().unwrap_or_default(),
        })
    }

    fn client_id(&self) -> Option<String> {
        self.oidc_client_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
    }

    /// `APP_OIDC_EXTRA_PARAMS`' `key=value` pairs; the error says which isn't one.
    fn extra_params(&self) -> Result<BTreeMap<String, String>, String> {
        self.oidc_extra_params
            .split_whitespace()
            .map(|pair| match pair.split_once('=') {
                Some((key, value)) if !key.is_empty() => Ok((key.to_owned(), value.to_owned())),
                _ => Err(format!("has `{pair}`, which isn't a `key=value` pair")),
            })
            .collect()
    }

    /// A `RedisBus` when `APP_REDIS_URL` is set. Unset, nothing is broadcast,
    /// which a server or worker refuses when `required`: a module that
    /// broadcasts would otherwise work nowhere and say so nowhere.
    pub fn bus(&self, required: bool) -> Result<Arc<dyn Bus>, Box<figment::Error>> {
        match self.redis_url.clone().filter(|url| !url.is_empty()) {
            Some(url) => Ok(Arc::new(RedisBus::new(
                url,
                self.redis_ca.clone().filter(|ca| !ca.is_empty()),
            ))),
            None if required => Err(Box::new(figment::Error::from(
                "refusing to start without APP_REDIS_URL: a module broadcasts between \
                 instances over the Redis or Valkey they share"
                    .to_string(),
            ))),
            None => Ok(Arc::new(NoBus)),
        }
    }

    /// `APP_BOOTSTRAP_ADMINS`, each trimmed, without empty entries.
    pub fn bootstrap_admins(&self) -> Vec<String> {
        self.bootstrap_admins
            .split(',')
            .map(str::trim)
            .filter(|subject| !subject.is_empty())
            .map(str::to_owned)
            .collect()
    }

    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn is_prod(&self) -> bool {
        self.env == Environment::Prod
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prod() -> AppConfig {
        AppConfig {
            env: Environment::Prod,
            ..AppConfig::default()
        }
    }

    #[test]
    fn dev_default_is_accepted_outside_prod() {
        assert!(AppConfig::default().validate().is_ok());
    }

    #[test]
    fn prod_rejects_the_builtin_dev_secret() {
        assert!(prod().validate().is_err());
    }

    #[test]
    fn prod_accepts_a_replaced_secret() {
        let config = AppConfig {
            jwt_secret: "a-real-deployment-secret".to_string(),
            ..prod()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn prod_accepts_jwks_mode_since_the_secret_is_unused() {
        let config = AppConfig {
            jwt_issuer: Some("https://issuer.example.com".to_string()),
            ..prod()
        };
        assert!(config.validate_serving(false).is_ok());
    }

    fn with_issuer(config: AppConfig) -> AppConfig {
        AppConfig {
            jwt_issuer: Some(ISSUER.to_string()),
            ..config
        }
    }

    const ISSUER: &str = "https://issuer.example.com/";

    #[test]
    fn prod_with_the_spa_embedded_refuses_an_issuer_without_a_client_id() {
        for client_id in [None, Some("  ".to_string())] {
            let config = AppConfig {
                oidc_client_id: client_id,
                ..with_issuer(prod())
            };
            let error = config.validate_serving(true).expect_err("refused");
            assert!(error.to_string().contains("APP_JWT_ISSUER"), "{error}");
            assert!(error.to_string().contains("APP_OIDC_CLIENT_ID"), "{error}");
        }
    }

    #[test]
    fn an_issuer_without_a_client_id_starts_outside_prod_or_without_the_spa() {
        assert!(with_issuer(prod()).validate_serving(false).is_ok());
        for env in [Environment::Dev, Environment::Test] {
            let config = AppConfig {
                env,
                ..with_issuer(AppConfig::default())
            };
            assert!(config.validate_serving(true).is_ok());
        }
    }

    #[test]
    fn prod_with_the_spa_embedded_starts_with_an_issuer_and_a_client_id() {
        let config = AppConfig {
            oidc_client_id: Some("spa".to_string()),
            ..with_issuer(prod())
        };
        assert!(config.validate_serving(true).is_ok());
    }

    #[test]
    fn without_an_issuer_there_is_no_sign_in() {
        let config = AppConfig {
            oidc_client_id: Some("spa".to_string()),
            ..AppConfig::default()
        };
        assert_eq!(config.sign_in(), None);
    }

    #[test]
    fn sign_in_requests_the_base_scopes_then_the_extra_ones_once_each() {
        let config = AppConfig {
            oidc_client_id: Some("spa".to_string()),
            oidc_scopes: " offline_access  email api://app/.default ".to_string(),
            oidc_extra_params: "audience=https://api.example.com prompt=login".to_string(),
            ..with_issuer(AppConfig::default())
        };
        assert_eq!(
            config.sign_in(),
            Some(SignIn {
                issuer: ISSUER.to_string(),
                client_id: Some("spa".to_string()),
                scopes: [
                    "openid",
                    "profile",
                    "email",
                    "offline_access",
                    "api://app/.default"
                ]
                .map(str::to_owned)
                .to_vec(),
                extra_params: BTreeMap::from([
                    (
                        "audience".to_string(),
                        "https://api.example.com".to_string()
                    ),
                    ("prompt".to_string(), "login".to_string()),
                ]),
            })
        );
    }

    #[test]
    fn an_extra_param_that_isnt_a_key_value_pair_refuses_to_start() {
        for params in ["audience", "=value", "audience=x prompt"] {
            let config = AppConfig {
                oidc_extra_params: params.to_string(),
                ..AppConfig::default()
            };
            let error = config.validate().expect_err(params);
            assert!(
                error.to_string().contains("APP_OIDC_EXTRA_PARAMS"),
                "{error}"
            );
        }
    }

    #[test]
    fn a_value_may_hold_an_equals_sign() {
        let config = AppConfig {
            oidc_extra_params: "claims={\"a\":1}=x".to_string(),
            ..AppConfig::default()
        };
        assert_eq!(
            config.extra_params(),
            Ok(BTreeMap::from([(
                "claims".to_string(),
                "{\"a\":1}=x".to_string()
            )]))
        );
    }

    #[test]
    fn the_sign_in_settings_are_read_as_strings_however_they_look() {
        let config: AppConfig = AppConfig::figment([
            ("oidc_client_id", "12345".to_string()),
            ("oidc_scopes", "true".to_string()),
            ("oidc_extra_params", "a=1".to_string()),
        ])
        .extract()
        .expect("extracts");
        assert_eq!(config.oidc_client_id.as_deref(), Some("12345"));
        assert_eq!(config.oidc_scopes, "true");
        assert_eq!(config.oidc_extra_params, "a=1");
    }

    #[test]
    fn a_server_whose_modules_broadcast_refuses_to_start_without_the_bus() {
        let error = AppConfig::default().bus(true).err().expect("refused");
        assert!(error.to_string().contains("APP_REDIS_URL"));
    }

    #[test]
    fn an_empty_bus_url_counts_as_none() {
        let config = AppConfig {
            redis_url: Some(String::new()),
            ..AppConfig::default()
        };
        assert!(config.bus(true).is_err());
    }

    #[test]
    fn bootstrap_admins_are_a_comma_separated_list() {
        let config = AppConfig {
            bootstrap_admins: " ada, ,auth0|bob ,".to_string(),
            ..AppConfig::default()
        };
        assert_eq!(config.bootstrap_admins(), ["ada", "auth0|bob"]);
        assert!(AppConfig::default().bootstrap_admins().is_empty());
    }

    #[test]
    fn a_bootstrap_admin_whose_subject_reads_as_a_number_or_bool_is_still_a_subject() {
        for raw in ["12345", "true", "109876543210987654321", "12345,ada"] {
            let config: AppConfig = AppConfig::figment([("bootstrap_admins", raw.to_string())])
                .extract()
                .unwrap_or_else(|err| panic!("{raw}: {err}"));
            assert_eq!(config.bootstrap_admins.as_str(), raw);
        }
    }

    #[test]
    fn a_server_whose_modules_dont_broadcast_needs_no_bus() {
        assert!(AppConfig::default().bus(false).is_ok());
    }

    #[test]
    fn a_configured_bus_is_used_whether_or_not_it_is_required() {
        let config = AppConfig {
            redis_url: Some("redis://localhost:6379".to_string()),
            ..AppConfig::default()
        };
        assert!(config.bus(true).is_ok());
        assert!(config.bus(false).is_ok());
    }
}
