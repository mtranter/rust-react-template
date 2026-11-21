use rig_core::providers::anthropic::wire::{ANTHROPIC, AnthropicConfig, Auth, Dialect};

/// Anthropic's own API taking its credential as `Authorization: Bearer`, as an OAuth token or a
/// gateway's bearer token must be sent, in place of `x-api-key`.
const ANTHROPIC_BEARER: Dialect = {
    let mut dialect = ANTHROPIC;
    dialect.quirks.auth = Auth::OptionalBearer;
    dialect
};

/// Anthropic's API refuses an OAuth access token on a request without this beta.
const OAUTH_BETA: &str = "oauth-2025-04-20";

/// A credential for Anthropic's API, read as Anthropic's SDKs read theirs: `ANTHROPIC_API_KEY`,
/// else `ANTHROPIC_AUTH_TOKEN` (an OAuth or other bearer token), with `ANTHROPIC_BASE_URL`, if
/// set, in place of `https://api.anthropic.com`.
pub enum AnthropicAuth {
    ApiKey(String),
    Bearer(String),
}

impl AnthropicAuth {
    /// `None` without either variable set, or with both empty.
    pub fn from_env() -> Option<Self> {
        let var = |name| std::env::var(name).ok();
        Self::from_vars(var("ANTHROPIC_API_KEY"), var("ANTHROPIC_AUTH_TOKEN"))
    }

    fn from_vars(api_key: Option<String>, token: Option<String>) -> Option<Self> {
        let given = |value: Option<String>| value.filter(|value| !value.trim().is_empty());
        given(api_key)
            .map(Self::ApiKey)
            .or_else(|| given(token).map(Self::Bearer))
    }

    pub(super) fn config(self) -> AnthropicConfig {
        let config = match self {
            Self::ApiKey(key) => AnthropicConfig::new(key),
            Self::Bearer(token) => {
                AnthropicConfig::with_key(&ANTHROPIC_BEARER, token).with_beta(OAUTH_BETA)
            }
        };
        match std::env::var("ANTHROPIC_BASE_URL") {
            Ok(base_url) if !base_url.trim().is_empty() => config.with_base_url(base_url),
            _ => config,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given(value: &str) -> Option<String> {
        Some(value.to_string())
    }

    #[test]
    fn neither_variable_is_no_credential() {
        assert!(AnthropicAuth::from_vars(None, None).is_none());
        assert!(AnthropicAuth::from_vars(given(" "), given("")).is_none());
    }

    #[test]
    fn an_api_key_goes_as_x_api_key_ahead_of_a_token() {
        let config = AnthropicAuth::from_vars(given("key"), given("token"))
            .map(AnthropicAuth::config)
            .unwrap();

        assert_eq!(config.api_key.expose(), "key");
        assert_eq!(config.dialect.quirks.auth, Auth::ApiKey);
        assert!(config.betas.is_empty());
    }

    #[test]
    fn a_token_goes_as_a_bearer_with_the_oauth_beta() {
        let config = AnthropicAuth::from_vars(None, given("token"))
            .map(AnthropicAuth::config)
            .unwrap();

        assert_eq!(config.api_key.expose(), "token");
        assert_eq!(config.dialect.quirks.auth, Auth::OptionalBearer);
        assert_eq!(config.betas, vec![OAUTH_BETA.to_string()]);
    }
}
