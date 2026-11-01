//! The `Mailer` adapter the server uses: the mail analogue of `db/`, the only
//! code that knows how mail leaves. It only logs; swap in an SMTP or provider
//! client here, configured through `di.rs`.

use crate::application::{Mail, Mailer};
use app_error::AppResult;
use async_trait::async_trait;

pub struct LogMailer;

#[async_trait]
impl Mailer for LogMailer {
    async fn send(&self, mail: Mail) -> AppResult<()> {
        tracing::info!(
            to = mail.to,
            subject = mail.subject,
            "mail logged, not delivered"
        );
        Ok(())
    }
}
