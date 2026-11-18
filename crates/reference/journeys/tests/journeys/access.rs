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

/// Rows aren't checked against the `author` scope until platform ticket 0028.
#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_who_is_not_the_author_may_still_edit_a_note_for_now(pool: PgPool) {
    let app = App::seeded(pool).await;
    let bob = app.as_user(USER_ID_BOB);

    let edited = bob
        .edit_note(ALICE_NOTE_ID_SHOPPING_LIST, "Bob's list", "")
        .await
        .unwrap();

    assert_eq!(edited["authorId"], USER_ID_ALICE.to_string());
    assert_eq!(app.note(ALICE_NOTE_ID_SHOPPING_LIST).await, Ok(edited));
}
