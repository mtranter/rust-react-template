use crate::driver::{Notes, Refusal, USER};
use notes::seed::{
    self, ALICE_NOTE_ID_SHOPPING_LIST, NOTE_ID_ORPHANED, SHOPPING_LIST_ITEM_ID_MILK,
};
use notes_api::events::v1::{NoteCreated, NoteDeleted, NoteEdited, NoteItemAdded, NoteItemRemoved};
use sqlx::PgPool;
use users_api::seed::{USER_ID_ALICE, USER_ID_BOB};

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn notes_are_refused_to_a_caller_who_is_not_signed_in(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    notes.sign_out();

    assert_eq!(notes.try_list().await.err(), Some(Refusal::NotSignedIn));
    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Err(Refusal::NotSignedIn)
    );
    assert_eq!(
        notes
            .create_claiming_to_be(USER_ID_ALICE, "Groceries")
            .await,
        Err(Refusal::NotSignedIn)
    );
    assert!(notes.announced::<NoteCreated>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_admin_is_let_into_notes(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    notes.sign_in_as(USER_ID_BOB, &["admin"]);

    assert_eq!(notes.try_list().await.map(|page| page.total), Ok(3));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn another_users_note_is_not_found(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    notes.sign_in_as(USER_ID_BOB, &[USER]);

    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Err(Refusal::NotFound)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn another_users_note_is_not_found_to_change_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    notes.sign_in_as(USER_ID_BOB, &[USER]);

    assert_eq!(
        notes
            .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Bob's now", "")
            .await,
        Err(Refusal::NotFound)
    );
    assert_eq!(
        notes.delete(ALICE_NOTE_ID_SHOPPING_LIST, 1).await,
        Err(Refusal::NotFound)
    );
    assert_eq!(
        notes
            .add_item(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Bread")
            .await,
        Err(Refusal::NotFound)
    );
    assert_eq!(
        notes
            .remove_item(ALICE_NOTE_ID_SHOPPING_LIST, 1, SHOPPING_LIST_ITEM_ID_MILK)
            .await,
        Err(Refusal::NotFound)
    );

    notes.sign_in_as(USER_ID_ALICE, &[USER]);
    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(seed::alices_note().into())
    );
    assert!(notes.announced::<NoteEdited>().await.is_empty());
    assert!(notes.announced::<NoteDeleted>().await.is_empty());
    assert!(notes.announced::<NoteItemAdded>().await.is_empty());
    assert!(notes.announced::<NoteItemRemoved>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_admin_sees_and_changes_anyones_note(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    notes.sign_in_as(USER_ID_BOB, &["admin"]);

    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(seed::alices_note().into())
    );
    let edited = notes
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Checked by an admin", "")
        .await
        .unwrap();
    assert_eq!(edited["title"], "Checked by an admin");
    assert_eq!(edited["authorId"], USER_ID_ALICE.to_string());
    assert_eq!(notes.delete(NOTE_ID_ORPHANED, 1).await, Ok(()));
}
