use app_error::AppResult;
use async_trait::async_trait;

/// Sends mail: an external system, so tests use a Fake (spec §13). `mail/` holds
/// the adapter the server uses.
#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, mail: Mail) -> AppResult<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub body: String,
}
