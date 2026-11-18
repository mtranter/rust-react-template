use crate::driver::{Notes, Refusal};
use app_events::EventPayload;
use app_persistence::Version;
use app_test_support::{assert_json_exists, assert_json_matches};
use notes_api::events::v1::{self, NoteCreated};
use notes_api::seed::AUTHOR_ID_UNKNOWN;
use sqlx::PgPool;
use users_api::seed::{USER_ID_ALICE, USER_ID_BOB};

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_created_note_is_kept_with_its_title_trimmed(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let created = notes
        .create(USER_ID_ALICE, "  Groceries  ", "hi")
        .await
        .unwrap();

    // The id and creation time are generated, so only their presence is known.
    assert_json_exists!(created, "$.id");
    assert_json_exists!(created, "$.createdAt");
    assert_json_matches!(created, "$.title", "Groceries");
    assert_json_matches!(created, "$.body", "hi");
    assert_json_matches!(created, "$.authorId", USER_ID_ALICE.to_string());
    assert_eq!(notes.get(created.id()).await, Ok(created));
    assert_eq!(notes.list().await.total, 4);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn creating_a_note_announces_it(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let created = notes
        .create(USER_ID_ALICE, "Groceries", "hi")
        .await
        .unwrap();

    assert_eq!(
        notes.announced::<NoteCreated>().await,
        [(
            Version::FIRST,
            EventPayload::new(v1::Note {
                id: created.id().parse().unwrap(),
                author_id: USER_ID_ALICE,
                title: "Groceries".into(),
                body: "hi".into(),
                archived_at: None,
                items: vec![],
                created_at: Some(notes.now()),
            })
        )]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_note_needs_a_title(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.create(USER_ID_ALICE, "   ", "").await,
        Err(Refusal::invalid(["title"]))
    );
    assert_eq!(notes.list().await.total, 3);
    assert!(notes.announced::<NoteCreated>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_note_title_has_a_maximum_length(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let too_long = "x".repeat(201);
    assert_eq!(
        notes.create(USER_ID_ALICE, &too_long, "").await,
        Err(Refusal::invalid(["title"]))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_caller_whose_id_is_not_a_user_id_cannot_write_a_note(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.create("nobody", "Title", "").await,
        Err(Refusal::invalid([]))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_note_by_someone_who_is_not_a_user_is_refused(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.create(AUTHOR_ID_UNKNOWN, "Title", "").await,
        Err(Refusal::invalid([]))
    );
    assert_eq!(notes.list().await.total, 3);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_note_sent_without_its_title_is_malformed_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.create_without_a_title(USER_ID_ALICE).await,
        Err(Refusal::Malformed)
    );
    assert_eq!(notes.list().await.total, 3);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_note_is_written_by_the_caller_whatever_its_body_says(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    notes.sign_in_as(USER_ID_ALICE, &[crate::driver::USER]);

    let created = notes
        .create_claiming_to_be(USER_ID_BOB, "Not Bob's")
        .await
        .unwrap();

    assert_json_matches!(created, "$.authorId", USER_ID_ALICE.to_string());
}
