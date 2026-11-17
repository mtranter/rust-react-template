use crate::driver::{Refusal, Users};
use sqlx::PgPool;
use users::UserId;
use users::seed::{self, USER_ID_ALICE};

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_can_be_found_by_their_id(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(users.get(USER_ID_ALICE).await, Ok(seed::alice().into()));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_unknown_user_is_not_found(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(users.get(UserId::new()).await, Err(Refusal::NotFound));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_malformed_user_id_is_invalid(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(users.get("not-a-user-id").await, Err(Refusal::invalid([])));
}
