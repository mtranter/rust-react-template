use crate::driver::{Refusal, Users};
use app_events::EventPayload;
use app_test_support::{assert_json_exists, assert_json_matches};
use sqlx::PgPool;
use users::seed;
use users_api::events::v1::{self, UserRegistered};

#[sqlx::test(migrations = false)]
async fn a_registered_user_is_kept_with_normalized_details(pool: PgPool) {
    let users = Users::seeded(pool).await;

    let carol = users
        .register("  Carol@Example.com ", "  Carol  ")
        .await
        .unwrap();

    // The id and creation time are generated, so only their presence is known.
    assert_json_exists!(carol, "$.id");
    assert_json_exists!(carol, "$.createdAt");
    assert_json_matches!(carol, "$.email", "carol@example.com");
    assert_json_matches!(carol, "$.displayName", "Carol");
    assert_eq!(users.get(carol.id()).await, Ok(carol));
    assert_eq!(users.list().await.total, 3);
}

#[sqlx::test(migrations = false)]
async fn registering_announces_the_new_user(pool: PgPool) {
    let users = Users::seeded(pool).await;

    let carol = users.register("carol@example.com", "Carol").await.unwrap();

    let announced = users.announced_envelopes::<UserRegistered>().await;
    assert_eq!(announced.len(), 1);
    let event = &announced[0];
    assert_eq!(event.source, "/users");
    assert_eq!(event.topic, "users.user.v1");
    assert_eq!(event.event_type, "users.user_registered");
    assert_eq!(event.partition_key, carol.id());
    assert_eq!(event.entity_version, 1);
    assert_eq!(event.subject, carol.id());
    assert_eq!(event.time, users.now());
    assert_eq!(
        event.data,
        EventPayload::new(v1::User {
            id: carol.id().parse().unwrap(),
            email: "carol@example.com".into(),
            display_name: "Carol".into(),
        })
    );
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
    assert!(users.announced::<UserRegistered>().await.is_empty());
}

#[sqlx::test(migrations = false)]
async fn a_user_needs_a_valid_email(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(
        users.register("not-an-email", "Dee").await,
        Err(Refusal::invalid(["email"]))
    );
}

#[sqlx::test(migrations = false)]
async fn a_user_needs_a_display_name(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(
        users.register("dee@example.com", "   ").await,
        Err(Refusal::invalid(["displayName"]))
    );
    assert_eq!(users.list().await.total, 2);
}
