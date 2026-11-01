use crate::driver::{Refusal, Widgets};
use app_test_support::{assert_json_exists, assert_json_matches};
use sqlx::PgPool;

#[sqlx::test(migrations = false)]
async fn a_created_widget_is_kept_with_its_name_trimmed(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;

    let created = widgets.create("  Third  ").await.unwrap();

    // The id and creation time are generated, so only their presence is known.
    assert_json_exists!(created, "$.id");
    assert_json_exists!(created, "$.createdAt");
    assert_json_matches!(created, "$.name", "Third");
    assert_eq!(widgets.get(created.id()).await, Ok(created));
}

#[sqlx::test(migrations = false)]
async fn a_widget_needs_a_name(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    assert_eq!(widgets.create("   ").await, Err(Refusal::invalid(["name"])));
    assert_eq!(widgets.list().await.total, 2);
}
