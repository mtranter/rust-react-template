//! Finding users.

use crate::driver::{Refusal, Users};
use sqlx::PgPool;
use users::UserId;
use users::seed::{self, ALICE, BOB};

#[sqlx::test(migrations = false)]
async fn a_user_can_be_found_by_their_id(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(users.get(ALICE).await, Ok(seed::alice().into()));
}

#[sqlx::test(migrations = false)]
async fn users_are_listed_most_recent_first(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let page = users.list().await;
    assert_eq!(page.total, 2);
    assert_eq!(page.ids(), [BOB.to_string(), ALICE.to_string()]);
}

#[sqlx::test(migrations = false)]
async fn an_unknown_user_is_not_found(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(users.get(UserId::new()).await, Err(Refusal::NotFound));
}

#[sqlx::test(migrations = false)]
async fn a_malformed_user_id_is_invalid(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(users.get("not-a-user-id").await, Err(Refusal::Invalid));
}
