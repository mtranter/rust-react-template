use app_domain::Clock;
use app_error::{AppError, AppResult};
use app_persistence::{Outgoing, Transactions, Tx, TxBackend};
use async_trait::async_trait;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use std::sync::Arc;

/// Begins each [`Tx`] on the pool. The clock stamps the events and jobs it writes.
pub struct PgTransactions {
    pool: PgPool,
    clock: Arc<dyn Clock>,
}

impl PgTransactions {
    pub fn new(pool: PgPool, clock: Arc<dyn Clock>) -> Self {
        Self { pool, clock }
    }
}

#[async_trait]
impl Transactions for PgTransactions {
    async fn begin(&self) -> AppResult<Tx> {
        begin(&self.pool, self.clock.clone()).await
    }
}

pub(crate) async fn begin(pool: &PgPool, clock: Arc<dyn Clock>) -> AppResult<Tx> {
    let tx = pool.begin().await.map_err(AppError::internal)?;
    Ok(Tx::new(PgTx { tx, clock }))
}

struct PgTx {
    tx: Transaction<'static, Postgres>,
    clock: Arc<dyn Clock>,
}

/// The connection a repository runs a write on: `.execute(app_db::conn(tx))`.
pub fn conn(tx: &mut Tx) -> &mut PgConnection {
    &mut tx
        .backend::<PgTx>()
        .expect("every unit of work is begun by `app_db`")
        .tx
}

#[async_trait]
impl TxBackend for PgTx {
    async fn commit(self: Box<Self>, outgoing: Vec<Outgoing>) -> AppResult<()> {
        let PgTx { mut tx, clock } = *self;
        let now = clock.now();
        for message in outgoing {
            match message {
                Outgoing::Event {
                    event_type,
                    payload,
                } => sqlx::query!(
                    r#"
                    INSERT INTO messaging.outbox (event_type, payload, occurred_at)
                    VALUES ($1, $2, $3)
                    "#,
                    event_type,
                    payload,
                    now,
                )
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?,
                Outgoing::Job {
                    kind,
                    payload,
                    run_at,
                    max_attempts,
                } => sqlx::query!(
                    r#"
                    INSERT INTO messaging.job (queue, payload, run_at, max_attempts, created_at)
                    VALUES ($1, $2, $3, $4, $5)
                    "#,
                    kind,
                    payload,
                    run_at.unwrap_or(now),
                    max_attempts,
                    now,
                )
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?,
            };
        }
        tx.commit().await.map_err(AppError::internal)
    }
}
