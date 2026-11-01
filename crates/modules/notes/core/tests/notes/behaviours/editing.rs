use crate::driver::{Notes, Refusal, announced};
use app_events::EventPayload;
use app_test_support::assert_json_matches;
use notes::NoteId;
use notes::seed::{self, ALICES_NOTE};
use notes_api::events::v1::{self, NoteEdited};
use sqlx::PgPool;

#[sqlx::test(migrations = false)]
async fn editing_a_note_replaces_its_title_and_body(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let edited = notes
        .edit(ALICES_NOTE, 1, " Renamed ", "updated")
        .await
        .unwrap();

    assert_json_matches!(edited, "$.title", "Renamed");
    assert_json_matches!(edited, "$.body", "updated");
    assert_eq!(notes.get(ALICES_NOTE).await, Ok(edited));
}

#[sqlx::test(migrations = false)]
async fn editing_a_note_announces_it_as_it_now_is(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    notes
        .edit(ALICES_NOTE, 1, "Renamed", "updated")
        .await
        .unwrap();

    assert_eq!(
        notes.announced::<NoteEdited>().await,
        [EventPayload::new(v1::Note {
            title: "Renamed".into(),
            body: "updated".into(),
            ..announced(&seed::alices_note()).entity
        })]
    );
}

#[sqlx::test(migrations = false)]
async fn a_successful_edit_advances_the_version(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let edited = notes
        .edit(ALICES_NOTE, 1, "Renamed", "updated")
        .await
        .unwrap();

    assert_json_matches!(edited, "$.version", "2");
}

#[sqlx::test(migrations = false)]
async fn editing_keeps_the_author_and_creation_time(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let original = seed::alices_note();

    let edited = notes
        .edit(ALICES_NOTE, 1, "Renamed", "updated")
        .await
        .unwrap();

    assert_json_matches!(edited, "$.authorId", original.author_id().to_string());
    assert_json_matches!(edited, "$.createdAt", original.created_at().to_rfc3339());
}

#[sqlx::test(migrations = false)]
async fn an_edit_without_a_title_is_rejected_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.edit(ALICES_NOTE, 1, "", "updated").await,
        Err(Refusal::invalid(["title"]))
    );
    assert_eq!(notes.get(ALICES_NOTE).await, Ok(seed::alices_note().into()));
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

    assert_eq!(
        notes.edit(ALICES_NOTE, 99, "Renamed", "updated").await,
        Err(Refusal::Stale)
    );
    assert_eq!(notes.get(ALICES_NOTE).await, Ok(seed::alices_note().into()));
    assert!(notes.announced::<NoteEdited>().await.is_empty());
}

#[sqlx::test(migrations = false)]
async fn an_edit_without_a_precondition_is_refused_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    assert_eq!(
        notes
            .edit_without_precondition(ALICES_NOTE, "Renamed", "updated")
            .await,
        Err(Refusal::PreconditionRequired)
    );
    assert_eq!(notes.get(ALICES_NOTE).await, Ok(seed::alices_note().into()));
}

#[sqlx::test(migrations = false)]
async fn a_second_edit_must_use_the_version_the_first_produced(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    notes.edit(ALICES_NOTE, 1, "Once", "a").await.unwrap();

    assert_eq!(
        notes.edit(ALICES_NOTE, 1, "Twice", "b").await,
        Err(Refusal::Stale)
    );

    let again = notes.edit(ALICES_NOTE, 2, "Twice", "b").await.unwrap();
    assert_json_matches!(again, "$.version", "3");
    assert_json_matches!(again, "$.title", "Twice");
}
