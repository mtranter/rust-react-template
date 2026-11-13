use crate::driver::{App, Refusal};
use notes_api::seed::{ALICE_NOTE_ID_SHOPPING_LIST, BOB_NOTE_ID_BIKE_REPAIRS};
use sqlx::PgPool;
use users_api::seed::USER_ID_ALICE;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn deleting_an_account_archives_only_that_users_notes(pool: PgPool) {
    let app = App::seeded(pool).await;
    let alices = app.note(ALICE_NOTE_ID_SHOPPING_LIST).await.unwrap();
    let bobs = app.note(BOB_NOTE_ID_BIKE_REPAIRS).await.unwrap();

    app.as_user(USER_ID_ALICE).delete_account().await.unwrap();
    assert_eq!(
        app.as_user(USER_ID_ALICE).me().await,
        Err(Refusal::NotFound)
    );
    assert_eq!(
        app.note(ALICE_NOTE_ID_SHOPPING_LIST).await.as_ref(),
        Ok(&alices)
    );

    app.run_pending_events().await;

    assert_eq!(
        app.note(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(alices.archived(app.now()))
    );
    assert_eq!(app.note(BOB_NOTE_ID_BIKE_REPAIRS).await, Ok(bobs));
    assert!(app.given_up_on().await.is_empty());
}
