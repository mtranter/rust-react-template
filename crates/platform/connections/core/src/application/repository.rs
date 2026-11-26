use super::{Expected, ListQuery, Listed, Tx, Version, Versioned};
use crate::domain::{Connection, ConnectionKey};
use app_authz::Rows;
use app_error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionSortKey {
    Key,
    Label,
}

/// Reads run on the pool; writes run in the caller's unit of work. A Connection's secrets are
/// read and written beside its row.
#[async_trait::async_trait]
pub trait ConnectionRepository: Send + Sync {
    /// Declared or retired.
    async fn get(&self, key: &ConnectionKey) -> AppResult<Option<Versioned<Connection>>>;
    /// Every Connection stored, retired ones included, for `migrate` to declare against.
    async fn all(&self) -> AppResult<Vec<Versioned<Connection>>>;
    /// The declared Connections the caller may see, by key unless sorted otherwise.
    async fn list(
        &self,
        query: &ListQuery<ConnectionSortKey>,
        rows: &Rows,
    ) -> AppResult<Listed<Versioned<Connection>>>;
    /// [`Expected::Version`] updates only if the stored version matches, else
    /// `PreconditionFailed`. Returns the new version.
    async fn save(
        &self,
        tx: &mut Tx,
        connection: &Connection,
        expected: Expected,
    ) -> AppResult<Version>;
}
