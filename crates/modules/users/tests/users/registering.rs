//! Registering users.

use crate::driver::{Refusal, Users};
use app_test_support::{assert_json_exists, assert_json_matches};
use sqlx::PgPool;
use users::seed;

#[sqlx::test(migrations = false)]
async fn a_registered_user_is_kept_with_normalized_details(pool: PgPool) {
    let users = Users::seeded(pool).await;

    let carol = users
        .register("  Carol@Example.com ", "  Carol  ")
        .await
        .unwrap();

    // The id and creation time are generated, so only their presence is known.
    assert_json_exists!(carol, "$.id");
    assert_json_exists!(carol, "$.created_at");
    assert_json_matches!(carol, "$.email", "carol@example.com");
    assert_json_matches!(carol, "$.display_name", "Carol");
    assert_eq!(users.get(carol.id()).await, Ok(carol));
    assert_eq!(users.list().await.total, 3);
}

#[sqlx::test(migrations = false)]
async fn an_email_can_only_be_registered_once(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let taken = seed::alice().email().to_string();

    assert_eq!(
        users.register(&taken, "Another Alice").await,
        Err(Refusal::Conflict)
    );
    // Normalization applies before the uniqueness check.
    assert_eq!(
        users.register(&taken.to_uppercase(), "Another Alice").await,
        Err(Refusal::Conflict)
    );
    assert_eq!(users.list().await.total, 2);
}

#[sqlx::test(migrations = false)]
async fn a_user_needs_a_valid_email(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(
        users.register("not-an-email", "Dee").await,
        Err(Refusal::Invalid)
    );
}

#[sqlx::test(migrations = false)]
async fn a_user_needs_a_display_name(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(
        users.register("dee@example.com", "   ").await,
        Err(Refusal::Invalid)
    );
    assert_eq!(users.list().await.total, 2);
}
