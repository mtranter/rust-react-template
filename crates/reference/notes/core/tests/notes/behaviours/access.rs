use crate::driver::{Notes, Refusal, USER};
use notes::seed::ALICE_NOTE_ID_SHOPPING_LIST;
use notes_api::events::v1::{NoteCreated, NoteEdited};
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
async fn a_signed_in_caller_without_the_user_role_is_kept_out_of_notes(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    notes.sign_in_as(USER_ID_ALICE, &["guest"]);

    assert_eq!(notes.try_list().await.err(), Some(Refusal::Forbidden));
    assert_eq!(
        notes.edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Mine", "").await,
        Err(Refusal::Forbidden)
    );
    assert_eq!(
        notes
            .create_claiming_to_be(USER_ID_ALICE, "Groceries")
            .await,
        Err(Refusal::Forbidden)
    );
    assert!(notes.announced::<NoteCreated>().await.is_empty());
    assert!(notes.announced::<NoteEdited>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_admin_is_let_into_notes(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    notes.sign_in_as(USER_ID_BOB, &["admin"]);

    assert_eq!(notes.try_list().await.map(|page| page.total), Ok(3));
}

/// Until rows are checked against the `author` scope (platform ticket 0028),
/// a `user` may edit any note.
#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_who_is_not_the_author_may_still_edit_a_note_for_now(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    notes.sign_in_as(USER_ID_BOB, &[USER]);

    let edited = notes
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Bob's now", "")
        .await
        .unwrap();

    assert_eq!(edited["title"], "Bob's now");
}
