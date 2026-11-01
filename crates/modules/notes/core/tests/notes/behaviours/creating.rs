use crate::driver::{Notes, Refusal};
use app_events::EventPayload;
use app_test_support::{assert_json_exists, assert_json_matches};
use notes_api::events::v1::{self, NoteCreated};
use notes_api::seed::UNKNOWN_AUTHOR;
use sqlx::PgPool;
use users_api::seed::ALICE;

#[sqlx::test(migrations = false)]
async fn a_created_note_is_kept_with_its_title_trimmed(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let created = notes.create(ALICE, "  Groceries  ", "hi").await.unwrap();

    // The id and creation time are generated, so only their presence is known.
    assert_json_exists!(created, "$.id");
    assert_json_exists!(created, "$.createdAt");
    assert_json_matches!(created, "$.title", "Groceries");
    assert_json_matches!(created, "$.body", "hi");
    assert_json_matches!(created, "$.authorId", ALICE.to_string());
    assert_eq!(notes.get(created.id()).await, Ok(created));
    assert_eq!(notes.list().await.total, 4);
}

#[sqlx::test(migrations = false)]
async fn creating_a_note_announces_it(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let created = notes.create(ALICE, "Groceries", "hi").await.unwrap();

    assert_eq!(
        notes.announced::<NoteCreated>().await,
        [EventPayload::new(v1::Note {
            id: created.id().parse().unwrap(),
            author_id: ALICE,
            title: "Groceries".into(),
            body: "hi".into(),
            archived_at: None,
        })]
    );
}

#[sqlx::test(migrations = false)]
async fn a_note_needs_a_title(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.create(ALICE, "   ", "").await,
        Err(Refusal::invalid(["title"]))
    );
    assert_eq!(notes.list().await.total, 3);
    assert!(notes.announced::<NoteCreated>().await.is_empty());
}

#[sqlx::test(migrations = false)]
async fn a_note_title_has_a_maximum_length(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let too_long = "x".repeat(201);
    assert_eq!(
        notes.create(ALICE, &too_long, "").await,
        Err(Refusal::invalid(["title"]))
    );
}

#[sqlx::test(migrations = false)]
async fn a_note_needs_a_valid_author(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.create("nobody", "Title", "").await,
        Err(Refusal::invalid(["authorId"]))
    );
}

#[sqlx::test(migrations = false)]
async fn a_note_by_someone_who_is_not_a_user_is_refused(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.create(UNKNOWN_AUTHOR, "Title", "").await,
        Err(Refusal::invalid(["authorId"]))
    );
    assert_eq!(notes.list().await.total, 3);
}

#[sqlx::test(migrations = false)]
async fn a_note_sent_without_its_title_is_malformed_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.create_without_a_title(ALICE).await,
        Err(Refusal::Malformed)
    );
    assert_eq!(notes.list().await.total, 3);
}
