use super::{Expected, Tx, Version, Versioned};
use crate::domain::Theme;
use app_error::AppResult;

/// Reads run on the pool; writes run in the caller's unit of work.
#[async_trait::async_trait]
pub trait ThemeRepository: Send + Sync {
    /// `None` until it is first saved.
    async fn get(&self) -> AppResult<Option<Versioned<Theme>>>;
    /// [`Expected::New`] inserts the first; [`Expected::Version`] updates only if the stored
    /// version matches. Either refused as `PreconditionFailed` when the stored one differs.
    async fn save(&self, tx: &mut Tx, theme: &Theme, expected: Expected) -> AppResult<Version>;
}
