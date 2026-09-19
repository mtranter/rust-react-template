//! Creating notes.

use crate::driver::{Notes, Refusal};
use app_test_support::{assert_json_exists, assert_json_matches};
use notes::seed::AUTHOR;
use sqlx::PgPool;

#[sqlx::test(migrations = false)]
async fn a_created_note_is_kept_with_its_title_trimmed(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let created = notes.create(AUTHOR, "  Third  ", "hi").await.unwrap();

    // The id and creation time are generated, so only their presence is known.
    assert_json_exists!(created, "$.id");
    assert_json_exists!(created, "$.created_at");
    assert_json_matches!(created, "$.title", "Third");
    assert_json_matches!(created, "$.body", "hi");
    assert_json_matches!(created, "$.author_id", AUTHOR.to_string());
    assert_eq!(notes.get(created.id()).await, Ok(created));
    assert_eq!(notes.list().await.total, 3);
}

#[sqlx::test(migrations = false)]
async fn a_note_needs_a_title(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(notes.create(AUTHOR, "   ", "").await, Err(Refusal::Invalid));
    assert_eq!(notes.list().await.total, 2);
}

#[sqlx::test(migrations = false)]
async fn a_note_title_has_a_maximum_length(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let too_long = "x".repeat(201);
    assert_eq!(
        notes.create(AUTHOR, &too_long, "").await,
        Err(Refusal::Invalid)
    );
}

#[sqlx::test(migrations = false)]
async fn a_note_needs_a_valid_author(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.create("nobody", "Title", "").await,
        Err(Refusal::Invalid)
    );
}
