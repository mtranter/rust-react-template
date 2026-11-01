use app_error::{AppError, AppResult};
use app_persistence::{Transactions, Tx, TxBackend};
use async_trait::async_trait;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};

/// Begins each [`Tx`] on the pool.
pub struct PgTransactions {
    pool: PgPool,
}

impl PgTransactions {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Transactions for PgTransactions {
    async fn begin(&self) -> AppResult<Tx> {
        begin(&self.pool).await
    }
}

pub(crate) async fn begin(pool: &PgPool) -> AppResult<Tx> {
    let tx = pool.begin().await.map_err(AppError::internal)?;
    Ok(Tx::new(PgTx(tx)))
}

struct PgTx(Transaction<'static, Postgres>);

/// The connection a repository runs a write on: `.execute(app_db::conn(tx))`.
pub fn conn(tx: &mut Tx) -> &mut PgConnection {
    &mut tx
        .backend::<PgTx>()
        .expect("every unit of work is begun by `app_db`")
        .0
}

#[async_trait]
impl TxBackend for PgTx {
    async fn commit(self: Box<Self>) -> AppResult<()> {
        self.0.commit().await.map_err(AppError::internal)
    }
}
