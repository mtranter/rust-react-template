//! A unit of work (spec §4): everything a use case writes, its aggregates and the
//! events it publishes, commits together or not at all. Pure: `app-db` supplies
//! the transaction behind [`Tx`], so `application/` never names the driver.

use app_error::AppResult;
use async_trait::async_trait;
use std::any::Any;

/// Begins units of work. Services hold an `Arc<dyn Transactions>`.
#[async_trait]
pub trait Transactions: Send + Sync {
    async fn begin(&self) -> AppResult<Tx>;
}

/// One open transaction. Repository writes and published events take `&mut Tx`;
/// dropping it without [`commit`](Self::commit), as `?` does on an error, rolls
/// everything back.
pub struct Tx {
    backend: Box<dyn TxBackend>,
}

/// The database transaction behind a [`Tx`]; implemented only by `app-db`.
#[async_trait]
pub trait TxBackend: Any + Send {
    async fn commit(self: Box<Self>) -> AppResult<()>;
}

impl Tx {
    pub fn new(backend: impl TxBackend) -> Self {
        Self {
            backend: Box::new(backend),
        }
    }

    pub async fn commit(self) -> AppResult<()> {
        self.backend.commit().await
    }

    /// The concrete transaction, for the db layer (`app_db::conn`).
    pub fn backend<B: TxBackend>(&mut self) -> Option<&mut B> {
        let backend: &mut dyn Any = &mut *self.backend;
        backend.downcast_mut()
    }
}
