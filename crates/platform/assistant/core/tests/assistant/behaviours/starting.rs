use crate::driver::{ALICE, Assistant, Refusal, announced};
use app_persistence::Version;
use assistant::{Conversation, ConversationId};
use assistant_api::events::v1::ConversationStarted;
use serde_json::json;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_started_conversation_is_the_callers_empty_and_untitled(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    let started = assistant.start().await.unwrap();

    assert_eq!(
        *started,
        json!({
            "id": started.id(),
            "title": "",
            "createdAt": assistant.now().to_rfc3339(),
            "messages": [],
            "version": "1",
        })
    );
    assert_eq!(assistant.get(started.id()).await, Ok(started));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn starting_a_conversation_announces_it(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    let started = assistant.start().await.unwrap();

    let conversation = Conversation::start(
        started.id().parse::<ConversationId>().unwrap(),
        ALICE.into(),
        assistant.now(),
    )
    .0;
    assert_eq!(
        assistant.announced::<ConversationStarted>().await,
        [(Version::FIRST, announced(&conversation))]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_caller_who_may_not_chat_cannot_start_one(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.revoke_chat();

    assert_eq!(assistant.start().await, Err(Refusal::Forbidden));
    assert!(
        assistant
            .announced::<ConversationStarted>()
            .await
            .is_empty()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_caller_who_is_not_signed_in_cannot_start_one(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.sign_out();

    assert_eq!(assistant.start().await, Err(Refusal::NotSignedIn));
    assert!(
        assistant
            .announced::<ConversationStarted>()
            .await
            .is_empty()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn conversations_can_be_started_while_the_assistant_is_off(pool: PgPool) {
    let assistant = Assistant::configured(pool, assistant::Settings::default).await;

    assert!(assistant.start().await.is_ok());
}
