use crate::driver::seeded_directory;
use sqlx::PgPool;
use users::seed::USER_ID_ALICE;
use uuid::Uuid;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_registered_user_exists(pool: PgPool) {
    let directory = seeded_directory(pool).await;
    assert!(
        directory
            .exists(USER_ID_ALICE.as_uuid())
            .await
            .expect("lookup succeeds")
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_unknown_user_does_not_exist(pool: PgPool) {
    let directory = seeded_directory(pool).await;
    assert!(
        !directory
            .exists(Uuid::new_v4())
            .await
            .expect("lookup succeeds")
    );
}
