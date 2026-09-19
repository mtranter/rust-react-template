//! Editing notes, including optimistic-concurrency control.

use crate::driver::{Notes, Refusal};
use app_test_support::assert_json_matches;
use notes::NoteId;
use notes::seed::{self, FIRST_NOTE};
use sqlx::PgPool;

#[sqlx::test(migrations = false)]
async fn editing_a_note_replaces_its_title_and_body(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let edited = notes
        .edit(FIRST_NOTE, 1, " Renamed ", "updated")
        .await
        .unwrap();

    assert_json_matches!(edited, "$.title", "Renamed");
    assert_json_matches!(edited, "$.body", "updated");
    assert_eq!(notes.get(FIRST_NOTE).await, Ok(edited));
}

#[sqlx::test(migrations = false)]
async fn a_successful_edit_advances_the_version(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let edited = notes
        .edit(FIRST_NOTE, 1, "Renamed", "updated")
        .await
        .unwrap();

    assert_json_matches!(edited, "$.version", "2");
}

#[sqlx::test(migrations = false)]
async fn editing_keeps_the_author_and_creation_time(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let original = seed::first_note();

    let edited = notes
        .edit(FIRST_NOTE, 1, "Renamed", "updated")
        .await
        .unwrap();

    assert_json_matches!(edited, "$.author_id", original.author_id().to_string());
    assert_json_matches!(edited, "$.created_at", original.created_at().to_rfc3339());
}

#[sqlx::test(migrations = false)]
async fn an_edit_without_a_title_is_rejected_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.edit(FIRST_NOTE, 1, "", "updated").await,
        Err(Refusal::Invalid)
    );
    assert_eq!(notes.get(FIRST_NOTE).await, Ok(seed::first_note().into()));
}

#[sqlx::test(migrations = false)]
async fn editing_an_unknown_note_is_not_found(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.edit(NoteId::new(), 1, "Title", "").await,
        Err(Refusal::NotFound)
    );
}

#[sqlx::test(migrations = false)]
async fn an_edit_against_a_stale_version_is_refused_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    // The note is at version 1; claiming any other version means the caller last
    // read a state that no longer holds.
    assert_eq!(
        notes.edit(FIRST_NOTE, 99, "Renamed", "updated").await,
        Err(Refusal::Stale)
    );
    assert_eq!(notes.get(FIRST_NOTE).await, Ok(seed::first_note().into()));
}

#[sqlx::test(migrations = false)]
async fn an_edit_without_a_precondition_is_refused_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    assert_eq!(
        notes
            .edit_without_precondition(FIRST_NOTE, "Renamed", "updated")
            .await,
        Err(Refusal::PreconditionRequired)
    );
    assert_eq!(notes.get(FIRST_NOTE).await, Ok(seed::first_note().into()));
}

#[sqlx::test(migrations = false)]
async fn a_second_edit_must_use_the_version_the_first_produced(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    // First edit succeeds against version 1 and moves the note to version 2.
    notes.edit(FIRST_NOTE, 1, "Once", "a").await.unwrap();

    // Re-using the now-stale version 1 is refused...
    assert_eq!(
        notes.edit(FIRST_NOTE, 1, "Twice", "b").await,
        Err(Refusal::Stale)
    );

    // ...but the current version is accepted and advances it again.
    let again = notes.edit(FIRST_NOTE, 2, "Twice", "b").await.unwrap();
    assert_json_matches!(again, "$.version", "3");
    assert_json_matches!(again, "$.title", "Twice");
}
