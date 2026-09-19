//! Typed application configuration, layered by `figment`.
//!
//! Pure crate: usable by any layer including the domain. Layering order (later
//! wins): built-in defaults → optional `config.toml` → `APP_*` env vars →
//! `DATABASE_URL` (honoured under its conventional name, not `APP_DATABASE_URL`).

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

/// Deployment environment. Drives log formatting and CORS strictness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Dev,
    Test,
    Prod,
}

/// The built-in HS256 secret. A clear dev-only marker; [`AppConfig::validate`]
/// refuses to start in prod while this is still in effect.
const DEV_JWT_SECRET: &str = "dev-only-insecure-secret-change-me";

/// Fully-resolved application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Bind host for the HTTP server.
    pub host: String,
    /// Bind port for the HTTP server.
    pub port: u16,
    /// Deployment environment.
    pub env: Environment,
    /// Postgres connection string.
    pub database_url: String,
    /// Maximum size of the Postgres connection pool.
    pub db_max_connections: u32,
    /// Allowed browser origin for CORS (dev: the Vite origin).
    pub cors_origin: String,
    /// HS256 secret for the JWT example. Used only when `jwt_issuer` is unset.
    /// Replace for real deployments.
    pub jwt_secret: String,
    /// OIDC issuer base URL. When set, tokens are verified against the provider's
    /// published keys (JWKS), discovered from
    /// `{issuer}/.well-known/openid-configuration`, instead of the HS256 secret.
    /// Leave unset for the self-contained dev path.
    pub jwt_issuer: Option<String>,
    /// Expected `aud` claim, validated when JWKS verification is active. Unset
    /// disables audience checking.
    pub jwt_audience: Option<String>,
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
        }
    }
}

impl AppConfig {
    /// Load configuration from the layered sources described in the module docs,
    /// then [`validate`](Self::validate) it.
    ///
    /// The error is boxed: `figment::Error` is large, and this is a cold path.
    pub fn load() -> Result<Self, Box<figment::Error>> {
        let config: Self = Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::file("config.toml"))
            .merge(Env::prefixed("APP_"))
            // DATABASE_URL is a de-facto standard name; map it to `database_url`.
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

    /// Reject configurations that are unsafe for the declared environment. Called
    /// by [`load`](Self::load); fails fast so an insecure prod deploy never boots.
    ///
    /// The one check today: in prod the built-in dev JWT secret must have been
    /// replaced (either set `APP_JWT_SECRET`, or use JWKS via `APP_JWT_ISSUER`,
    /// which makes the secret unused). Without this, a copy-paste deployment would
    /// silently accept tokens any attacker can forge.
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

    /// The `host:port` socket string.
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
        // The built-in secret is fine for the default (dev) environment.
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
}
