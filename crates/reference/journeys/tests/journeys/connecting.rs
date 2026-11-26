use crate::driver::{App, Refusal};
use connections_api::{Auth, Declaration};
use notes_api::seed::ALICE_NOTE_ID_SHOPPING_LIST;
use sqlx::PgPool;
use users_api::seed::{USER_ID_ALICE, USER_ID_BOB};

const TOKEN: &str = "tok_live_0123456789abcdef";

fn crm() -> Declaration {
    Declaration::new("crm", "CRM").auth(Auth::Bearer)
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_admin_sets_a_connections_url_and_secret_and_reads_back_that_it_is_set_but_not_its_value(
    pool: PgPool,
) {
    let app = App::seeded(pool).await;
    app.declares(crm()).await;
    // The seeds assign Alice `admin`.
    let admin = app.as_user(USER_ID_ALICE);

    let declared = admin.connection("crm").await.unwrap();
    let configured = admin
        .set_connection_url(&declared, "https://crm.example/api")
        .await
        .unwrap();
    admin
        .set_connection_secret(&configured, "token", TOKEN)
        .await
        .unwrap();

    let read = admin.connection("crm").await.unwrap();
    assert_eq!(read["url"], "https://crm.example/api");
    assert_eq!(read["secrets"][0]["name"], "token");
    assert_eq!(read["secrets"][0]["set"], true);
    assert_eq!(read["secrets"][0]["setBy"], USER_ID_ALICE.to_string());
    assert!(!read.to_string().contains(TOKEN));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn once_an_admin_connects_the_noticeboard_a_note_shared_is_pinned_there(pool: PgPool) {
    let app = App::seeded(pool).await;
    let board = fake_noticeboard::Noticeboard::new(TOKEN);
    let url = board.serve();
    let admin = app.as_user(USER_ID_ALICE);

    let declared = admin.connection(notes::NOTICEBOARD).await.unwrap();
    assert_eq!(declared["status"], "not_configured");
    let configured = admin.set_connection_url(&declared, &url).await.unwrap();
    admin
        .set_connection_secret(&configured, "token", TOKEN)
        .await
        .unwrap();
    let tested = admin.test_connection(notes::NOTICEBOARD).await.unwrap();
    assert_eq!(tested["status"], "ok");

    admin.share_note(ALICE_NOTE_ID_SHOPPING_LIST).await.unwrap();
    app.run_pending_events().await;

    let pin = &board.pins()[&ALICE_NOTE_ID_SHOPPING_LIST.to_string()];
    assert_eq!(pin.title, "Shopping list");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_who_is_not_an_admin_is_refused_a_connection(pool: PgPool) {
    let app = App::seeded(pool).await;
    app.declares(crm()).await;
    let declared = app.as_user(USER_ID_ALICE).connection("crm").await.unwrap();
    let bob = app.as_user(USER_ID_BOB);

    assert_eq!(bob.connection("crm").await, Err(Refusal::Forbidden));
    assert_eq!(
        bob.set_connection_secret(&declared, "token", TOKEN).await,
        Err(Refusal::Forbidden)
    );
    assert_eq!(
        app.as_user(USER_ID_ALICE).connection("crm").await,
        Ok(declared)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_agent_is_offered_a_connection_to_read_but_never_its_secrets_to_set(pool: PgPool) {
    let app = App::seeded(pool).await;
    let agent = app.as_agent(USER_ID_ALICE).await;

    let tools = agent.tools().await;

    assert!(tools.contains(&"get_connection".to_owned()));
    for secret in ["set_connection_secret", "clear_connection_secret"] {
        assert!(!tools.contains(&secret.to_owned()), "{secret} is a tool");
    }
}
