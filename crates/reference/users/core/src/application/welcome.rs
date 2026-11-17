use super::{Mail, Mailer, Tx, UserRepository};
use crate::domain::UserId;
use app_error::AppResult;
use app_events::EnvelopeOf;
use app_persistence::Handle;
use async_trait::async_trait;
use std::sync::Arc;
use users_api::events::v1::UserRegistered;

/// Users reacting to its own event, as any subscriber would: a user is welcomed
/// if and only if their registration commits, and a mail server that's down
/// only delays it.
pub struct Welcome {
    repo: Arc<dyn UserRepository>,
    mailer: Arc<dyn Mailer>,
}

impl Welcome {
    pub fn new(repo: Arc<dyn UserRepository>, mailer: Arc<dyn Mailer>) -> Self {
        Self { repo, mailer }
    }
}

#[async_trait]
impl Handle<UserRegistered> for Welcome {
    async fn handle(&self, _tx: &mut Tx, event: EnvelopeOf<UserRegistered>) -> AppResult<()> {
        let user = event.data.entity;
        // Deleted before this ran: there's nobody to welcome.
        if self.repo.get(UserId::from_uuid(user.id)).await?.is_none() {
            return Ok(());
        }
        self.mailer
            .send(Mail {
                to: user.email,
                subject: "Welcome".into(),
                body: format!("Hi {}, welcome aboard.", user.display_name),
            })
            .await
    }
}
