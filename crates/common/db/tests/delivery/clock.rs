use crate::support::Messaging;
use app_db::Worker;
use app_domain::{SystemClock, seed_time};
use app_error::AppError;
use app_persistence::Handlers;
use sqlx::PgPool;
use std::sync::Arc;

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_worker_whose_clock_is_far_from_the_databases_fails_the_check(pool: PgPool) {
    let messaging = Messaging::new(pool).await;

    let checked = messaging.worker(&[]).check_clock().await;

    let Err(AppError::Internal(cause)) = checked else {
        panic!("a clock at seed time passed: {checked:?}");
    };
    let cause = cause.to_string();
    assert!(cause.contains(&seed_time(0).to_string()), "{cause}");
    assert!(cause.contains("s behind the database's"), "{cause}");
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn a_worker_on_the_real_clock_passes_the_check(pool: PgPool) {
    let worker = Worker::new(pool, Arc::new(SystemClock), Handlers::new());

    worker.check_clock().await.unwrap();
}
