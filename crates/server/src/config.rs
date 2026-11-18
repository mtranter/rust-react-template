//! Layering (later wins): defaults → optional `config.toml` → `APP_*` env vars →
//! `DATABASE_URL` (its conventional name, not `APP_DATABASE_URL`).

use app_bus::{Bus, NoBus, RedisBus};
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Dev,
    Test,
    Prod,
}

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
        }
    }
}

impl AppConfig {
    /// The error is boxed: `figment::Error` is large, and this is a cold path.
    pub fn load() -> Result<Self, Box<figment::Error>> {
        let config: Self = Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::file("config.toml"))
            .merge(Env::prefixed("APP_"))
            .merge(
                Env::raw()
                    .only(&["DATABASE_URL"])
                    .map(|key| key.as_str().to_ascii_lowercase().into()),
            )
            .extract()
            .map_err(Box::new)?;
        config.validate()?;
        Ok(config)
    }

    /// Without this, a copy-paste prod deploy would accept tokens anyone can forge.
    pub fn validate(&self) -> Result<(), Box<figment::Error>> {
        if self.is_prod() && self.jwt_issuer.is_none() && self.jwt_secret == DEV_JWT_SECRET {
            return Err(Box::new(figment::Error::from(
                "refusing to start in prod with the built-in dev JWT secret: \
                 set APP_JWT_SECRET, or configure APP_JWT_ISSUER for JWKS verification"
                    .to_string(),
            )));
        }
        Ok(())
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
        assert!(config.validate().is_ok());
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
