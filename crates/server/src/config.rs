//! Layering (later wins): defaults → optional `config.toml` → `APP_*` env vars →
//! `DATABASE_URL` (its conventional name, not `APP_DATABASE_URL`).

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

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
}
