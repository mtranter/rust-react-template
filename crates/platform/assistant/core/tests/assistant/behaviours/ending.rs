use crate::driver::{Assistant, LIST_ID, Step, calls, calls_all, of_kind, says};
use assistant::seed::ALICE_CONVERSATION_ID_UNTITLED;
use assistant::{ModelReply, Settings};
use serde_json::{Value, json};
use sqlx::PgPool;

/// The kinds and texts of the conversation's messages, oldest first.
async fn messages(assistant: &Assistant) -> Vec<(String, String)> {
    let conversation = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    conversation["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let text = m["text"].as_str().unwrap_or_default().to_string();
            (m["kind"].as_str().unwrap().to_string(), text)
        })
        .collect()
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn reaching_the_tool_call_limit_ends_the_turn_saying_so(pool: PgPool) {
    let assistant = Assistant::limited(
        pool,
        Settings {
            max_tool_calls: 2,
            ..Settings::default()
        },
    )
    .await;
    let read = || ("get_list", json!({ "id": LIST_ID }));
    assistant.scripted([
        calls_all(&[read(), read()]),
        calls("get_list", json!({ "id": LIST_ID })),
    ]);

    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Read it over and over")
        .await
        .unwrap();

    assert_eq!(
        of_kind(&events, "toolResult").len(),
        2,
        "the third read never runs"
    );
    assert_eq!(events.last().unwrap()["kind"], "finished");
    let stored = messages(&assistant).await;
    assert_eq!(stored.len(), 3);
    assert_eq!(stored[2].0, "assistant");
    assert!(
        stored[2].1.contains("limit of 2 tool calls"),
        "{}",
        stored[2].1
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_reply_cut_short_at_the_token_limit_ends_the_turn_saying_so(pool: PgPool) {
    let assistant = Assistant::limited(
        pool,
        Settings {
            max_output_tokens: 50,
            ..Settings::default()
        },
    )
    .await;
    assistant.scripted([Step::Reply(ModelReply {
        text: "Here is a very long".into(),
        output_tokens: 50,
        truncated: true,
        ..ModelReply::default()
    })]);

    assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Tell me everything")
        .await
        .unwrap();

    let [request] = assistant.model_requests().try_into().unwrap();
    assert_eq!(request.max_output_tokens, 50);
    let stored = messages(&assistant).await;
    assert_eq!(stored.len(), 2);
    assert!(stored[1].1.starts_with("Here is a very long\n\n"));
    assert!(
        stored[1].1.contains("limit of 50 output tokens"),
        "{}",
        stored[1].1
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_token_limit_counts_every_step_of_the_turn(pool: PgPool) {
    let assistant = Assistant::limited(
        pool,
        Settings {
            max_output_tokens: 30,
            ..Settings::default()
        },
    )
    .await;
    // Each scripted step spends 10 tokens.
    assistant.scripted([
        calls("get_list", json!({ "id": LIST_ID })),
        calls("get_list", json!({ "id": LIST_ID })),
        calls("get_list", json!({ "id": LIST_ID })),
    ]);

    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Keep reading")
        .await
        .unwrap();

    let budgets: Vec<u32> = assistant
        .model_requests()
        .iter()
        .map(|request| request.max_output_tokens)
        .collect();
    assert_eq!(budgets, [30, 20, 10]);
    assert_eq!(of_kind(&events, "toolResult").len(), 2);
    let stored = messages(&assistant).await;
    assert!(
        stored
            .last()
            .unwrap()
            .1
            .contains("limit of 30 output tokens")
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_failing_model_ends_the_turn_with_an_error_that_is_stored(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.scripted([Step::Fail("ThrottlingException: slow down".into())]);

    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Hello?")
        .await
        .unwrap();

    let [error] = of_kind(&events, "error").try_into().unwrap();
    assert_eq!(error["error"]["code"], "unavailable");
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("ThrottlingException")
    );
    assert_eq!(events.last().unwrap()["kind"], "finished");
    let stored = messages(&assistant).await;
    assert_eq!(stored[0], ("user".to_string(), "Hello?".to_string()));
    assert_eq!(stored[1].0, "error");
    assert!(stored[1].1.contains("ThrottlingException"));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_error_is_never_sent_to_the_model(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.scripted([Step::Fail("AccessDeniedException".into()), says("Hi!")]);
    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Hello?")
        .await
        .unwrap();
    let version: i64 = events.last().unwrap()["version"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, version, "Hello again?")
        .await
        .unwrap();

    let requests = assistant.model_requests();
    assert_eq!(
        requests[1].messages,
        [
            assistant::ModelMessage::User("Hello?".into()),
            assistant::ModelMessage::User("Hello again?".into()),
        ]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_token_the_app_refuses_mid_turn_ends_it_saying_so(pool: PgPool) {
    let assistant = Assistant::signed_out_of_the_app(pool).await;
    assistant.scripted([calls_all(&[
        ("get_list", json!({ "id": LIST_ID })),
        ("create_list", json!({ "body": {} })),
    ])]);

    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Read my note")
        .await
        .unwrap();

    let [result] = of_kind(&events, "toolResult").try_into().unwrap();
    assert_eq!(result["error"]["code"], "unauthorized");
    let [error] = of_kind(&events, "error").try_into().unwrap();
    assert_eq!(error["error"]["code"], "unauthorized");
    assert!(of_kind(&events, "approvalRequired").is_empty());
    assert_eq!(
        assistant.model_requests().len(),
        1,
        "the model isn't asked again"
    );
    let conversation = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    let calls: Vec<Value> = conversation["messages"][1]["toolCalls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| call["status"].clone())
        .collect();
    assert_eq!(
        calls,
        [json!("ran"), json!("rejected")],
        "nothing is left to approve"
    );
    assert_eq!(conversation["messages"][2]["kind"], "error");
    assert!(
        conversation["messages"][2]["text"]
            .as_str()
            .unwrap()
            .contains("sign-in expired")
    );
}
