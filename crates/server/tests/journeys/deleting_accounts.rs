use crate::driver::{App, Refusal};
use notes_api::seed::{ALICES_NOTE, BOBS_NOTE};
use sqlx::PgPool;
use users_api::seed::ALICE;

#[sqlx::test(migrations = false)]
async fn deleting_an_account_archives_only_that_users_notes(pool: PgPool) {
    let app = App::seeded(pool).await;
    let alices = app.note(ALICES_NOTE).await.unwrap();
    let bobs = app.note(BOBS_NOTE).await.unwrap();

    app.as_user(ALICE).delete_account().await.unwrap();
    assert_eq!(app.as_user(ALICE).me().await, Err(Refusal::NotFound));
    assert_eq!(app.note(ALICES_NOTE).await.as_ref(), Ok(&alices));

    app.run_pending_events().await;

    assert_eq!(app.note(ALICES_NOTE).await, Ok(alices.archived(app.now())));
    assert_eq!(app.note(BOBS_NOTE).await, Ok(bobs));
    assert!(app.given_up_on().await.is_empty());
}
