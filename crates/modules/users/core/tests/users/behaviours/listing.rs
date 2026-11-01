use crate::driver::{Refusal, Users};
use sqlx::PgPool;
use users::seed::{ALICE, BOB};

#[sqlx::test(migrations = false)]
async fn users_are_listed_most_recent_first(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let page = users.list().await;
    assert_eq!(page.total, 2);
    assert_eq!(page.ids(), [BOB.to_string(), ALICE.to_string()]);
}

#[sqlx::test(migrations = false)]
async fn users_can_be_sorted_by_display_name(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let page = users
        .list_sorted(&["displayName"])
        .await
        .expect("a known sort key");
    assert_eq!(page.ids(), [ALICE.to_string(), BOB.to_string()]);
}

#[sqlx::test(migrations = false)]
async fn users_can_be_sorted_by_email_descending(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let page = users
        .list_sorted(&["-email"])
        .await
        .expect("a known sort key");
    assert_eq!(page.ids(), [BOB.to_string(), ALICE.to_string()]);
}

#[sqlx::test(migrations = false)]
async fn sorting_by_an_unknown_key_is_malformed(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(
        users.list_sorted(&["password"]).await.err(),
        Some(Refusal::Malformed)
    );
}

#[sqlx::test(migrations = false)]
async fn a_page_holds_its_slice_and_counts_every_user(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let page = users.list_page(1, 1).await;
    assert_eq!(page.ids(), [ALICE.to_string()]);
    assert_eq!((page.total, page.limit, page.offset), (2, 1, 1));
}
