use crate::driver::{App, Refusal};
use sqlx::PgPool;
use users_api::seed::USER_ID_ALICE;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_agent_writes_a_note_and_edits_it(pool: PgPool) {
    let app = App::seeded(pool).await;
    let agent = app.as_agent(USER_ID_ALICE).await;

    let written = agent
        .write_note("Garden plan", "tomatoes", None)
        .await
        .unwrap();
    assert_eq!(app.note(written.id()).await.as_ref(), Ok(&written));

    let edited = agent
        .edit_note(
            &written,
            written.version(),
            "Garden plan",
            "tomatoes, beans",
        )
        .await
        .unwrap();
    assert_eq!(edited["body"], "tomatoes, beans");
    assert_eq!(edited.version(), written.version() + 1);
    assert_eq!(app.note(written.id()).await.as_ref(), Ok(&edited));
    assert_eq!(agent.note(written.id()).await, Ok(edited));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_agents_stale_edit_is_refused_and_changes_nothing(pool: PgPool) {
    let app = App::seeded(pool).await;
    let agent = app.as_agent(USER_ID_ALICE).await;
    let written = agent
        .write_note("Garden plan", "tomatoes", None)
        .await
        .unwrap();
    let edited = agent
        .edit_note(&written, written.version(), "Garden plan", "beans")
        .await
        .unwrap();

    assert_eq!(
        agent
            .edit_note(&written, written.version(), "Garden plan", "peas")
            .await,
        Err(Refusal::Stale)
    );
    assert_eq!(app.note(written.id()).await, Ok(edited));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_retried_keyed_create_creates_one_note(pool: PgPool) {
    let app = App::seeded(pool).await;
    let agent = app.as_agent(USER_ID_ALICE).await;

    let written = agent
        .write_note("Retried plan", "tomatoes", Some("plan-1"))
        .await
        .unwrap();
    let retried = agent
        .write_note("Retried plan", "tomatoes", Some("plan-1"))
        .await;

    assert_eq!(retried.as_ref(), Ok(&written));
    assert_eq!(
        app.search_notes("Retried plan").await.ids(),
        vec![written.id().to_owned()]
    );
    assert_eq!(
        agent
            .write_note("Another plan", "beans", Some("plan-1"))
            .await,
        Err(Refusal::KeyReused)
    );
    assert_eq!(
        app.search_notes("Another plan").await.ids(),
        Vec::<String>::new()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_agents_me_is_its_user(pool: PgPool) {
    let app = App::seeded(pool).await;
    let agent = app.as_agent(USER_ID_ALICE).await;

    assert_eq!(agent.me().await, app.as_user(USER_ID_ALICE).me().await);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_tokenless_agent_is_refused_a_session(pool: PgPool) {
    let app = App::seeded(pool).await;

    assert_eq!(app.agent_signed_out().await, Err(Refusal::NotSignedIn));
}
