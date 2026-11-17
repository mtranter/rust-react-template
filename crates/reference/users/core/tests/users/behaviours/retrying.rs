use crate::driver::Users;
use app_test_support::assert_json_matches;
use sqlx::PgPool;
use users::seed::{USER_ID_ALICE, USER_ID_BOB};
use users_api::events::v1::UserDisplayNameChanged;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn one_users_key_never_answers_another_users_identical_request(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let alices = users
        .signed_in_as(USER_ID_ALICE)
        .change_display_name_keyed("rename-me", 1, "Ace")
        .await
        .unwrap();

    let bobs = users
        .signed_in_as(USER_ID_BOB)
        .change_display_name_keyed("rename-me", 1, "Ace")
        .await
        .unwrap();

    assert_ne!(bobs, alices);
    assert_json_matches!(bobs, "$.displayName", "Ace");
    assert_eq!(users.signed_in_as(USER_ID_BOB).me().await, Ok(bobs));
    assert_eq!(users.announced::<UserDisplayNameChanged>().await.len(), 2);
}
