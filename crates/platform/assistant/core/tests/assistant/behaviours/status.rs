use crate::driver::{Assistant, Refusal, Status};
use assistant::Settings;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn without_a_model_configured_the_assistant_is_off(pool: PgPool) {
    let assistant = Assistant::configured(pool, Settings::default).await;

    assert_eq!(assistant.status().await, Ok(Status { on: false }));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn with_a_model_configured_the_assistant_is_on(pool: PgPool) {
    fn bedrock() -> Settings {
        Settings {
            model: Some("eu.anthropic.claude-sonnet-4-5-20250929-v1:0".to_string()),
            ..Settings::default()
        }
    }
    let assistant = Assistant::configured(pool, bedrock).await;

    assert_eq!(assistant.status().await, Ok(Status { on: true }));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn with_a_scripted_model_the_assistant_is_on(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    assert_eq!(assistant.status().await, Ok(Status { on: true }));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn whether_it_is_on_is_refused_to_a_caller_who_is_not_signed_in(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.sign_out();

    assert_eq!(assistant.status().await, Err(Refusal::NotSignedIn));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_whose_chat_is_revoked_is_refused(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.revoke_chat();

    assert_eq!(assistant.status().await, Err(Refusal::Forbidden));
}
