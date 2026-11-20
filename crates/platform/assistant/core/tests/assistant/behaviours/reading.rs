use crate::driver::{ADMIN, Assistant, BOB, Refusal};
use assistant::ConversationId;
use assistant::seed::{self, ALICE_CONVERSATION_ID_ROLES, BOB_CONVERSATION_ID_GREETING};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_conversation_reads_with_its_messages_and_tool_calls_oldest_first(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    assert_eq!(
        assistant.get(ALICE_CONVERSATION_ID_ROLES).await,
        Ok(seed::alices_roles_conversation().into())
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn another_users_conversation_is_not_found(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    assert_eq!(
        assistant.get(BOB_CONVERSATION_ID_GREETING).await,
        Err(Refusal::NotFound)
    );

    assistant.sign_in_as(BOB, &[]);
    assert_eq!(
        assistant.get(BOB_CONVERSATION_ID_GREETING).await,
        Ok(seed::bobs_conversation().into())
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_admin_cannot_read_another_users_conversation(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.sign_in_as(ADMIN, &["admin"]);

    assert_eq!(
        assistant.get(ALICE_CONVERSATION_ID_ROLES).await,
        Err(Refusal::NotFound)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_unknown_conversation_is_not_found(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    assert_eq!(
        assistant.get(ConversationId::new()).await,
        Err(Refusal::NotFound)
    );
    assert_eq!(assistant.get("not-an-id").await, Err(Refusal::Invalid));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_caller_who_may_not_chat_cannot_read_even_their_own(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.revoke_chat();

    assert_eq!(
        assistant.get(ALICE_CONVERSATION_ID_ROLES).await,
        Err(Refusal::Forbidden)
    );
}
