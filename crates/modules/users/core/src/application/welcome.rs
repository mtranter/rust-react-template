use super::{Mail, Mailer, Tx, UserRepository};
use crate::domain::UserId;
use app_error::AppResult;
use app_persistence::{Handle, Job};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Enqueued with the registration, so a user is welcomed if and only if their
/// registration commits, and a mail server that's down only delays it.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WelcomeEmail {
    pub user_id: Uuid,
}

impl Job for WelcomeEmail {
    const KIND: &'static str = "users.welcome_email";
}

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
impl Handle<WelcomeEmail> for Welcome {
    async fn handle(&self, _tx: &mut Tx, job: WelcomeEmail) -> AppResult<()> {
        // Deleted before the job ran: there's nobody to welcome.
        let Some(user) = self.repo.get(UserId::from_uuid(job.user_id)).await? else {
            return Ok(());
        };
        let user = user.entity;
        self.mailer
            .send(Mail {
                to: user.email().to_string(),
                subject: "Welcome".into(),
                body: format!("Hi {}, welcome aboard.", user.display_name()),
            })
            .await
    }
}
