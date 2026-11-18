use crate::driver::{App, Refusal};
use notes::seed::ALICE_NOTE_ID_SHOPPING_LIST;
use sqlx::PgPool;
use users_api::seed::{USER_ID_ALICE, USER_ID_BOB};

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_token_without_the_user_role_is_refused_by_notes(pool: PgPool) {
    let app = App::seeded(pool).await;
    let guest = app.with_roles(USER_ID_ALICE, &["guest"]);

    assert_eq!(guest.notes().await.err(), Some(Refusal::Forbidden));
    assert_eq!(
        guest.write_note("Groceries", "milk").await,
        Err(Refusal::Forbidden)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_agent_without_the_user_role_is_refused_by_notes(pool: PgPool) {
    let app = App::seeded(pool).await;
    let agent = app.as_agent_with_roles(USER_ID_ALICE, &["guest"]).await;

    assert_eq!(
        agent.write_note("Garden plan", "tomatoes", None).await,
        Err(Refusal::Forbidden)
    );
    assert_eq!(
        agent.note(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Err(Refusal::Forbidden)
    );
}

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
