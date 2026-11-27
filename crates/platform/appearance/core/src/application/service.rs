use super::{EventPublisher, Expected, ThemeRepository, Transactions, Version, Versioned, events};
use crate::domain::{Overrides, Theme, ThemeAction};
use app_authz::{Authorizer, Principal};
use app_domain::Clock;
use app_error::{AppError, AppResult};
use std::sync::Arc;

#[derive(Clone)]
pub struct ThemeService {
    repo: Arc<dyn ThemeRepository>,
    publisher: Arc<dyn EventPublisher>,
    transactions: Arc<dyn Transactions>,
    clock: Arc<dyn Clock>,
    authorizer: Arc<Authorizer>,
}

pub struct UpdateTheme {
    pub light: Overrides,
    pub dark: Overrides,
    pub shared: Overrides,
}

impl ThemeService {
    pub fn new(
        repo: Arc<dyn ThemeRepository>,
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

    /// Public: `None` (version 0, the defaults) until first saved.
    pub async fn get(&self) -> AppResult<Option<Versioned<Theme>>> {
        self.repo.get().await
    }

    /// `expected` is the version the caller last read (from `If-Match`), `0` for one never saved.
    pub async fn update(
        &self,
        principal: &Principal,
        cmd: UpdateTheme,
        expected: Version,
    ) -> AppResult<Versioned<Theme>> {
        self.authorizer
            .authorize::<Theme>(principal, ThemeAction::Update)?;
        let by = principal.id().unwrap_or("system").to_owned();
        let (theme, event) = Theme::save(cmd.light, cmd.dark, cmd.shared, by, self.clock.now())?;
        let expected = match expected.as_i64() {
            0 => Expected::New,
            _ => Expected::Version(expected),
        };
        let mut tx = self.transactions.begin().await?;
        let version = self.repo.save(&mut tx, &theme, expected).await?;
        self.publisher
            .publish(&mut tx, events::pending(&theme, version, event))
            .await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: theme,
            version,
        })
    }
}

pub fn stale_error() -> AppError {
    AppError::precondition_failed("the theme was changed by someone else")
}
