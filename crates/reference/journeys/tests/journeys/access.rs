use crate::driver::{App, Refusal};
use notes::seed::ALICE_NOTE_ID_SHOPPING_LIST;
use sqlx::PgPool;
use users_api::seed::USER_ID_BOB;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_who_is_not_the_author_is_refused_a_note_as_if_it_did_not_exist(pool: PgPool) {
    let app = App::seeded(pool).await;
    let bob = app.as_user(USER_ID_BOB);
    let before = app.note(ALICE_NOTE_ID_SHOPPING_LIST).await;

    assert_eq!(
        bob.edit_note(ALICE_NOTE_ID_SHOPPING_LIST, "Bob's list", "")
            .await,
        Err(Refusal::NotFound)
    );
    assert!(
        bob.notes()
            .await
            .unwrap()
            .ids()
            .iter()
            .all(|id| *id != ALICE_NOTE_ID_SHOPPING_LIST.to_string())
    );
    assert_eq!(app.note(ALICE_NOTE_ID_SHOPPING_LIST).await, before);
}
