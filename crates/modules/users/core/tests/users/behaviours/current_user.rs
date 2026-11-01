use crate::driver::{Refusal, Users};
use app_test_support::assert_json_matches;
use sqlx::PgPool;
use users::UserId;
use users::seed::{self, ALICE, BOB};

#[sqlx::test(migrations = false)]
async fn a_signed_in_user_sees_their_own_profile(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(
        users.signed_in_as(ALICE).me().await,
        Ok(seed::alice().into())
    );
    assert_eq!(users.signed_in_as(BOB).me().await, Ok(seed::bob().into()));
}

#[sqlx::test(migrations = false)]
async fn viewing_your_profile_requires_signing_in(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(users.anonymous().me().await, Err(Refusal::NotSignedIn));
}

#[sqlx::test(migrations = false)]
async fn a_signed_in_identity_with_no_user_is_not_found(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let stranger = users.signed_in_as(UserId::new());
    assert_eq!(stranger.me().await, Err(Refusal::NotFound));
    assert_eq!(
        stranger.change_display_name(1, "Stranger").await,
        Err(Refusal::NotFound)
    );
}

#[sqlx::test(migrations = false)]
async fn a_user_can_change_their_display_name(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let alice = users.signed_in_as(ALICE);

    let changed = alice.change_display_name(1, "  Alice B  ").await.unwrap();

    assert_json_matches!(changed, "$.displayName", "Alice B");
    assert_eq!(users.get(ALICE).await, Ok(changed));
}

#[sqlx::test(migrations = false)]
async fn a_successful_change_advances_the_version(pool: PgPool) {
    let users = Users::seeded(pool).await;

    let changed = users
        .signed_in_as(ALICE)
        .change_display_name(1, "Alice B")
        .await
        .unwrap();

    assert_json_matches!(changed, "$.version", "2");
}

#[sqlx::test(migrations = false)]
async fn changing_display_name_keeps_email_and_creation_time(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let original = seed::alice();

    let changed = users
        .signed_in_as(ALICE)
        .change_display_name(1, "Alice B")
        .await
        .unwrap();

    assert_json_matches!(changed, "$.email", original.email().to_string());
    assert_json_matches!(changed, "$.createdAt", original.created_at().to_rfc3339());
}

#[sqlx::test(migrations = false)]
async fn changing_display_name_affects_only_that_user(pool: PgPool) {
    let users = Users::seeded(pool).await;
    users
        .signed_in_as(ALICE)
        .change_display_name(1, "Alice B")
        .await
        .unwrap();
    assert_eq!(users.get(BOB).await, Ok(seed::bob().into()));
}

#[sqlx::test(migrations = false)]
async fn a_blank_display_name_is_rejected_and_changes_nothing(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(
        users
            .signed_in_as(ALICE)
            .change_display_name(1, "   ")
            .await,
        Err(Refusal::invalid(["displayName"]))
    );
    assert_eq!(users.get(ALICE).await, Ok(seed::alice().into()));
}

#[sqlx::test(migrations = false)]
async fn changing_display_name_requires_signing_in(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(
        users.anonymous().change_display_name(1, "Anyone").await,
        Err(Refusal::NotSignedIn)
    );
    assert_eq!(users.get(ALICE).await, Ok(seed::alice().into()));
}

#[sqlx::test(migrations = false)]
async fn a_change_against_a_stale_version_is_refused_and_changes_nothing(pool: PgPool) {
    let users = Users::seeded(pool).await;

    // Seeded accounts are at version 1.
    assert_eq!(
        users
            .signed_in_as(ALICE)
            .change_display_name(99, "Alice B")
            .await,
        Err(Refusal::Stale)
    );
    assert_eq!(users.get(ALICE).await, Ok(seed::alice().into()));
}

#[sqlx::test(migrations = false)]
async fn a_change_without_a_precondition_is_refused_and_changes_nothing(pool: PgPool) {
    let users = Users::seeded(pool).await;

    assert_eq!(
        users
            .signed_in_as(ALICE)
            .change_display_name_without_precondition("Alice B")
            .await,
        Err(Refusal::PreconditionRequired)
    );
    assert_eq!(users.get(ALICE).await, Ok(seed::alice().into()));
}

#[sqlx::test(migrations = false)]
async fn a_change_from_a_stale_tab_is_refused_after_a_change_elsewhere(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let alice = users.signed_in_as(ALICE);

    // Both tabs loaded Alice at version 1.
    let from_one = alice
        .change_display_name(1, "Alice from tab one")
        .await
        .unwrap();
    assert_json_matches!(from_one, "$.version", "2");

    assert_eq!(
        alice.change_display_name(1, "Alice from tab two").await,
        Err(Refusal::Stale)
    );

    let from_two = alice
        .change_display_name(2, "Alice from tab two")
        .await
        .unwrap();
    assert_json_matches!(from_two, "$.displayName", "Alice from tab two");
    assert_json_matches!(from_two, "$.version", "3");
}
