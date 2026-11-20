use crate::driver::{ADMIN, Assistant, BOB, Refusal, announced};
use app_persistence::Version;
use assistant::seed::{self, ALICE_CONVERSATION_ID_ROLES, BOB_CONVERSATION_ID_GREETING};
use assistant_api::events::v1::ConversationDeleted;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_deleted_conversation_is_gone_with_its_messages(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    assistant
        .delete(ALICE_CONVERSATION_ID_ROLES, 1)
        .await
        .unwrap();

    assert_eq!(
        assistant.get(ALICE_CONVERSATION_ID_ROLES).await,
        Err(Refusal::NotFound)
    );
    assert!(
        !assistant
            .list()
            .await
            .unwrap()
            .ids()
            .contains(&ALICE_CONVERSATION_ID_ROLES.to_string())
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn deleting_a_conversation_announces_it_as_it_was_last(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    assistant
        .delete(ALICE_CONVERSATION_ID_ROLES, 1)
        .await
        .unwrap();

    assert_eq!(
        assistant.announced::<ConversationDeleted>().await,
        [(
            Version::from_i64(2),
            announced(&seed::alices_roles_conversation())
        )]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_delete_against_a_stale_version_is_refused_and_changes_nothing(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    assert_eq!(
        assistant.delete(ALICE_CONVERSATION_ID_ROLES, 2).await,
        Err(Refusal::Stale)
    );
    assert_eq!(
        assistant.get(ALICE_CONVERSATION_ID_ROLES).await,
        Ok(seed::alices_roles_conversation().into())
    );
    assert!(
        assistant
            .announced::<ConversationDeleted>()
            .await
            .is_empty()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_delete_without_a_precondition_is_refused_and_changes_nothing(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    assert_eq!(
        assistant
            .delete_without_precondition(ALICE_CONVERSATION_ID_ROLES)
            .await,
        Err(Refusal::PreconditionRequired)
    );
    assert_eq!(
        assistant.get(ALICE_CONVERSATION_ID_ROLES).await,
        Ok(seed::alices_roles_conversation().into())
    );
    assert!(
        assistant
            .announced::<ConversationDeleted>()
            .await
            .is_empty()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn another_users_conversation_cannot_be_deleted(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    assert_eq!(
        assistant.delete(BOB_CONVERSATION_ID_GREETING, 1).await,
        Err(Refusal::NotFound)
    );
    assistant.sign_in_as(ADMIN, &["admin"]);
    assert_eq!(
        assistant.delete(BOB_CONVERSATION_ID_GREETING, 1).await,
        Err(Refusal::NotFound)
    );

    assistant.sign_in_as(BOB, &[]);
    assert_eq!(
        assistant.get(BOB_CONVERSATION_ID_GREETING).await,
        Ok(seed::bobs_conversation().into())
    );
    assert!(
        assistant
            .announced::<ConversationDeleted>()
            .await
            .is_empty()
    );
}
