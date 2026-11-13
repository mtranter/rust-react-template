use crate::driver::{Notes, Refusal, announced};
use app_events::EventPayload;
use app_persistence::Version;
use app_test_support::assert_json_matches;
use notes::NoteId;
use notes::seed::{self, ALICE_NOTE_ID_SHOPPING_LIST};
use notes_api::events::v1::{self, NoteEdited};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn editing_a_note_replaces_its_title_and_body(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let edited = notes
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, " Renamed ", "updated")
        .await
        .unwrap();

    assert_json_matches!(edited, "$.title", "Renamed");
    assert_json_matches!(edited, "$.body", "updated");
    assert_eq!(notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await, Ok(edited));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn editing_a_note_announces_it_as_it_now_is(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    notes
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Renamed", "updated")
        .await
        .unwrap();

    assert_eq!(
        notes.announced::<NoteEdited>().await,
        [(
            Version::from_i64(2),
            EventPayload::new(v1::Note {
                title: "Renamed".into(),
                body: "updated".into(),
                ..announced(&seed::alices_note()).entity
            })
        )]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_successful_edit_advances_the_version(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let edited = notes
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Renamed", "updated")
        .await
        .unwrap();

    assert_json_matches!(edited, "$.version", "2");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn editing_keeps_the_author_and_creation_time(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let original = seed::alices_note();

    let edited = notes
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Renamed", "updated")
        .await
        .unwrap();

    assert_json_matches!(edited, "$.authorId", original.author_id().to_string());
    assert_json_matches!(edited, "$.createdAt", original.created_at().to_rfc3339());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_edit_without_a_title_is_rejected_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes
            .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "", "updated")
            .await,
        Err(Refusal::invalid(["title"]))
    );
    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(seed::alices_note().into())
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn editing_an_unknown_note_is_not_found(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.edit(NoteId::new(), 1, "Title", "").await,
        Err(Refusal::NotFound)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_edit_against_a_stale_version_is_refused_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    assert_eq!(
        notes
            .edit(ALICE_NOTE_ID_SHOPPING_LIST, 99, "Renamed", "updated")
            .await,
        Err(Refusal::Stale)
    );
    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(seed::alices_note().into())
    );
    assert!(notes.announced::<NoteEdited>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_edit_without_a_precondition_is_refused_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    assert_eq!(
        notes
            .edit_without_precondition(ALICE_NOTE_ID_SHOPPING_LIST, "Renamed", "updated")
            .await,
        Err(Refusal::PreconditionRequired)
    );
    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(seed::alices_note().into())
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_second_edit_must_use_the_version_the_first_produced(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    notes
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Once", "a")
        .await
        .unwrap();

    assert_eq!(
        notes
            .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Twice", "b")
            .await,
        Err(Refusal::Stale)
    );

    let again = notes
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 2, "Twice", "b")
        .await
        .unwrap();
    assert_json_matches!(again, "$.version", "3");
    assert_json_matches!(again, "$.title", "Twice");
}
