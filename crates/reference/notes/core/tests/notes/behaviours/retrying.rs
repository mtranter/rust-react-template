use crate::driver::{NoteJson, Notes, Outcome, Refusal};
use notes::seed::ALICE_NOTE_ID_SHOPPING_LIST;
use notes_api::events::v1::{NoteCreated, NoteDeleted, NoteEdited};
use sqlx::PgPool;
use users_api::seed::USER_ID_ALICE;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_retried_create_gets_the_original_answer_and_creates_nothing_more(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let keyed = notes.with_idempotency_key("create-groceries");
    let original = keyed.create(USER_ID_ALICE, "Groceries", "milk").await;

    let retried = keyed.create(USER_ID_ALICE, "Groceries", "milk").await;

    assert_eq!(retried, original);
    let created: NoteJson = original.json();
    assert_eq!(notes.get(created.id()).await, Ok(created));
    assert_eq!(notes.announced::<NoteCreated>().await.len(), 1);
    assert_eq!(notes.list().await.total, 2);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_retried_edit_gets_the_original_answer_rather_than_a_stale_refusal(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let keyed = notes.with_idempotency_key("rename-alices-note");
    let original = keyed
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Renamed", "updated")
        .await;

    let retried = keyed
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Renamed", "updated")
        .await;

    assert_eq!(retried, original);
    let edited: NoteJson = original.json();
    assert_eq!(edited.version(), 2);
    assert_eq!(notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await, Ok(edited));
    assert_eq!(notes.announced::<NoteEdited>().await.len(), 1);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_retried_delete_gets_the_original_answer_rather_than_not_found(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let keyed = notes.with_idempotency_key("delete-alices-note");
    let original = keyed.delete(ALICE_NOTE_ID_SHOPPING_LIST, 1).await;

    let retried = keyed.delete(ALICE_NOTE_ID_SHOPPING_LIST, 1).await;

    assert_eq!(retried, original);
    assert_eq!(original.outcome::<(), Refusal>(), Ok(()));
    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Err(Refusal::NotFound)
    );
    assert_eq!(notes.announced::<NoteDeleted>().await.len(), 1);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_key_sent_again_with_a_different_request_is_refused_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let keyed = notes.with_idempotency_key("create-groceries");
    keyed.create(USER_ID_ALICE, "Groceries", "milk").await;

    let reused: Outcome<NoteJson> = keyed
        .create(USER_ID_ALICE, "Chores", "laundry")
        .await
        .outcome();

    assert_eq!(reused, Err(Refusal::KeyReused));
    assert_eq!(notes.announced::<NoteCreated>().await.len(), 1);
    assert_eq!(notes.list().await.total, 2);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_key_sent_again_expecting_another_version_is_a_different_request(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let keyed = notes.with_idempotency_key("rename-alices-note");
    keyed
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Renamed", "updated")
        .await;

    let reused: Outcome<NoteJson> = keyed
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 2, "Renamed", "updated")
        .await
        .outcome();

    assert_eq!(reused, Err(Refusal::KeyReused));
    assert_eq!(notes.announced::<NoteEdited>().await.len(), 1);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_failed_request_keeps_no_answer_so_its_retry_runs_again(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let keyed = notes.with_idempotency_key("rename-alices-note");
    notes.saving_fails_for(ALICE_NOTE_ID_SHOPPING_LIST).await;
    let failed = keyed
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Renamed", "updated")
        .await;
    notes.saving_works_again().await;

    let retried = keyed
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Renamed", "updated")
        .await;

    assert_ne!(retried, failed);
    let edited: NoteJson = retried.json();
    assert_eq!(notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await, Ok(edited));
    assert_eq!(notes.announced::<NoteEdited>().await.len(), 1);
}
