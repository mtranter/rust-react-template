use crate::driver::{Refusal, USER, Users};
use app_authz::Principal;
use app_db::Columns;
use sqlx::PgPool;
use users::seed::{USER_ID_ALICE, USER_ID_BOB};
use users::{UserColumns, seed};

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn users_are_listed_most_recent_first(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let page = users.list().await;
    assert_eq!(page.total, 2);
    assert_eq!(
        page.ids(),
        [USER_ID_BOB.to_string(), USER_ID_ALICE.to_string()]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn users_can_be_sorted_by_display_name(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let page = users
        .list_sorted(&["displayName"])
        .await
        .expect("a known sort key");
    assert_eq!(
        page.ids(),
        [USER_ID_ALICE.to_string(), USER_ID_BOB.to_string()]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn users_can_be_sorted_by_email_descending(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let page = users
        .list_sorted(&["-email"])
        .await
        .expect("a known sort key");
    assert_eq!(
        page.ids(),
        [USER_ID_BOB.to_string(), USER_ID_ALICE.to_string()]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn sorting_by_an_unknown_key_is_malformed(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(
        users.list_sorted(&["password"]).await.err(),
        Some(Refusal::Malformed)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_page_holds_its_slice_and_counts_every_user(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let page = users.list_page(1, 1).await;
    assert_eq!(page.ids(), [USER_ID_ALICE.to_string()]);
    assert_eq!((page.total, page.limit, page.offset), (2, 1, 1));
}

#[test]
fn every_account_attribute_has_a_column() {
    UserColumns::assert_every_attribute_is_mapped();
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_scoped_query_holds_exactly_the_accounts_each_plan_allows(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let principals = [
        Principal::user(USER_ID_ALICE.to_string(), [USER]),
        Principal::user(USER_ID_BOB.to_string(), [USER]),
        Principal::user("nobody", [USER]),
        Principal::user(USER_ID_BOB.to_string(), ["admin"]),
    ];

    app_authz::testing::assert_lists_agree(
        &users.authorizer(),
        &principals,
        &seed::users(),
        |user| user.id().as_uuid(),
        |scope| users.ids_where::<UserColumns>(scope),
    )
    .await;
}
