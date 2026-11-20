use crate::driver::{ADMIN, Assistant, BOB, Refusal};
use assistant::seed::{
    ALICE_CONVERSATION_ID_ROLES, ALICE_CONVERSATION_ID_UNTITLED, BOB_CONVERSATION_ID_GREETING,
};
use serde_json::json;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_lists_only_their_own_conversations_newest_first(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    let page = assistant.list().await.unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(
        page.ids(),
        [
            ALICE_CONVERSATION_ID_UNTITLED.to_string(),
            ALICE_CONVERSATION_ID_ROLES.to_string(),
        ]
    );

    assistant.sign_in_as(BOB, &[]);
    assert_eq!(
        assistant.list().await.unwrap().ids(),
        [BOB_CONVERSATION_ID_GREETING.to_string()]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_listed_conversation_has_its_title_but_not_its_messages(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    let page = assistant.list().await.unwrap();
    assert_eq!(
        **page.item(ALICE_CONVERSATION_ID_ROLES),
        json!({
            "id": ALICE_CONVERSATION_ID_ROLES.to_string(),
            "title": "Which roles are there?",
            "createdAt": app_domain::seed_time(0).to_rfc3339(),
            "version": "1",
        })
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_started_conversation_is_listed_first(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    let started = assistant.start().await.unwrap();

    let page = assistant.list().await.unwrap();
    assert_eq!(page.ids()[0], started.id());
    assert_eq!(page.total, 3);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_admin_lists_only_their_own_conversations(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.sign_in_as(ADMIN, &["admin"]);

    let page = assistant.list().await.unwrap();
    assert!(page.ids().is_empty());
    assert_eq!(page.total, 0);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_page_and_its_total_count_only_the_callers_own(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    let page = assistant.list_page(1, 1).await.unwrap();
    assert_eq!(page.ids(), [ALICE_CONVERSATION_ID_ROLES.to_string()]);
    assert_eq!((page.total, page.limit, page.offset), (2, 1, 1));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn conversations_can_be_listed_oldest_first(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    assert_eq!(
        assistant.list_sorted(&["createdAt"]).await.unwrap().ids(),
        [
            ALICE_CONVERSATION_ID_ROLES.to_string(),
            ALICE_CONVERSATION_ID_UNTITLED.to_string(),
        ]
    );
    assert_eq!(
        assistant.list_sorted(&["title"]).await.err(),
        Some(Refusal::Malformed)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_caller_who_may_not_chat_cannot_list_them(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.revoke_chat();

    assert_eq!(assistant.list().await.err(), Some(Refusal::Forbidden));
}
