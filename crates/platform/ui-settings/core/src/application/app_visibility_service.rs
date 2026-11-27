use super::{
    AppVisibilityRepository, EventPublisher, Expected, Transactions, Version, Versioned, events,
};
use crate::domain::{AppVisibility, AppVisibilityAction};
use app_authz::{Authorizer, Principal};
use app_domain::Clock;
use app_error::{AppError, AppResult};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppVisibilityService {
    repo: Arc<dyn AppVisibilityRepository>,
    publisher: Arc<dyn EventPublisher>,
    transactions: Arc<dyn Transactions>,
    clock: Arc<dyn Clock>,
    authorizer: Arc<Authorizer>,
}

impl AppVisibilityService {
    pub fn new(
        repo: Arc<dyn AppVisibilityRepository>,
        publisher: Arc<dyn EventPublisher>,
        transactions: Arc<dyn Transactions>,
        clock: Arc<dyn Clock>,
        authorizer: Arc<Authorizer>,
    ) -> Self {
        Self {
            repo,
            publisher,
            transactions,
            clock,
            authorizer,
        }
    }

    /// Any signed-in caller's: `None` (version 0, nothing hidden) until first saved.
    pub async fn get(&self) -> AppResult<Option<Versioned<AppVisibility>>> {
        self.repo.get().await
    }

    /// `expected` is the version the caller last read (from `If-Match`), `0` for one never saved.
    pub async fn update(
        &self,
        principal: &Principal,
        hidden: Vec<String>,
        expected: Version,
    ) -> AppResult<Versioned<AppVisibility>> {
        self.authorizer
            .authorize::<AppVisibility>(principal, AppVisibilityAction::Update)?;
        let by = principal.id().unwrap_or("system").to_owned();
        let (visibility, event) = AppVisibility::save(hidden, by, self.clock.now())?;
        let expected = match expected.as_i64() {
            0 => Expected::New,
            _ => Expected::Version(expected),
        };
        let mut tx = self.transactions.begin().await?;
        let version = self.repo.save(&mut tx, &visibility, expected).await?;
        self.publisher
            .publish(
                &mut tx,
                events::app_visibility_pending(&visibility, version, event),
            )
            .await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: visibility,
            version,
        })
    }
}

pub fn app_visibility_stale_error() -> AppError {
    AppError::precondition_failed("which apps are hidden was changed by someone else")
}
