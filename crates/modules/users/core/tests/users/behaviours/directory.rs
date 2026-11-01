use crate::driver::seeded_directory;
use sqlx::PgPool;
use users::seed::ALICE;
use uuid::Uuid;

#[sqlx::test(migrations = false)]
async fn a_registered_user_exists(pool: PgPool) {
    let directory = seeded_directory(pool).await;
    assert!(
        directory
            .exists(ALICE.as_uuid())
            .await
            .expect("lookup succeeds")
    );
}

#[sqlx::test(migrations = false)]
async fn an_unknown_user_does_not_exist(pool: PgPool) {
    let directory = seeded_directory(pool).await;
    assert!(
        !directory
            .exists(Uuid::new_v4())
            .await
            .expect("lookup succeeds")
    );
}
