use crate::driver::{Assistant, BOB, Refusal, calls, calls_all, of_kind, says, streamed_text};
use assistant::seed::{
    ALICE_CONVERSATION_ID_ROLES, ALICE_CONVERSATION_ID_UNTITLED, LOOKUP_TOOL_CALL_ID_LIST_ROLES,
};
use assistant::{ModelMessage, ModelToolResult};
use assistant_api::events::v1::ConversationToolCallDecided;
use serde_json::{Value, json};
use sqlx::PgPool;

/// Has the model ask for `create_list` with `body` in Alice's untitled conversation: the id of
/// the call awaiting her decision.
async fn awaiting_a_write(assistant: &Assistant, body: Value) -> String {
    assistant.scripted([calls("create_list", json!({ "body": body }))]);
    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Write a list")
        .await
        .unwrap();
    of_kind(&events, "approvalRequired")[0]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn status_of(conversation: &Value, call: &str) -> Value {
    conversation["messages"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|message| message["toolCalls"].as_array().cloned().unwrap_or_default())
        .find(|c| c["id"] == call)
        .unwrap()
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_approved_write_runs_as_the_caller_once_and_the_turn_goes_on(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    let list = json!({ "title": "Groceries" });
    let call = awaiting_a_write(&assistant, list.clone()).await;
    let pending = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    assert_eq!(
        status_of(&pending, &call)["summary"],
        "Write a list, owned by the caller.",
        "a pending call says what approving it would do"
    );
    assistant.scripted([says("Done: your list is written.")]);

    let events = assistant
        .approve(ALICE_CONVERSATION_ID_UNTITLED, &call)
        .await
        .expect("the resumed turn streams");

    assert_eq!(
        assistant.written_with_keys(),
        vec![(list.clone(), Some(format!("assistant-tool-call-{call}")))],
        "run once, with a key derived from the call's id"
    );
    assert_eq!(
        events[0],
        json!({ "kind": "decided", "id": call, "status": "approved" })
    );
    assert_eq!(
        events[1],
        json!({ "kind": "toolResult", "id": call, "result": list })
    );
    assert_eq!(streamed_text(&events), "Done: your list is written.");
    let finished = events.last().unwrap();
    assert_eq!(finished["kind"], "finished");

    let conversation = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    let decided = status_of(&conversation, &call);
    assert_eq!(decided["status"], "approved");
    assert_eq!(decided["result"], list);
    assert_eq!(conversation["version"], finished["version"]);
    let answered = &assistant.model_requests()[1].messages[2];
    assert_eq!(
        answered,
        &ModelMessage::ToolResults(vec![ModelToolResult {
            id: call.clone(),
            name: "create_list".to_string(),
            content: list.to_string(),
            is_error: false,
        }])
    );
    let announced = assistant.announced::<ConversationToolCallDecided>().await;
    assert_eq!(announced.len(), 1);
    assert_eq!(announced[0].0.as_i64(), 4);
    assert_eq!(announced[0].1.context.tool_call_id.to_string(), call);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_rejected_write_doesnt_run_and_the_model_is_told_the_user_declined(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    let call = awaiting_a_write(&assistant, json!({ "title": "Groceries" })).await;
    assistant.scripted([says("All right, I won't.")]);

    let events = assistant
        .reject(ALICE_CONVERSATION_ID_UNTITLED, &call)
        .await
        .expect("the resumed turn streams");

    assert_eq!(assistant.written(), Vec::<Value>::new());
    assert_eq!(
        events[0],
        json!({ "kind": "decided", "id": call, "status": "rejected" })
    );
    assert!(of_kind(&events, "toolResult").is_empty());
    assert_eq!(streamed_text(&events), "All right, I won't.");
    let conversation = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    let decided = status_of(&conversation, &call);
    assert_eq!(decided["status"], "rejected");
    assert!(decided.get("result").is_none() && decided.get("error").is_none());
    assert_eq!(
        assistant.model_requests()[1].messages[2],
        ModelMessage::ToolResults(vec![ModelToolResult {
            id: call,
            name: "create_list".to_string(),
            content: "the user declined this action".to_string(),
            is_error: true,
        }])
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn approving_twice_writes_once_and_the_second_is_a_conflict(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    let call = awaiting_a_write(&assistant, json!({ "title": "Groceries" })).await;
    assistant.scripted([says("Done.")]);
    assistant
        .approve(ALICE_CONVERSATION_ID_UNTITLED, &call)
        .await
        .unwrap();
    let before = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();

    assert_eq!(
        assistant
            .approve(ALICE_CONVERSATION_ID_UNTITLED, &call)
            .await,
        Err(Refusal::Conflict)
    );
    assert_eq!(
        assistant
            .reject(ALICE_CONVERSATION_ID_UNTITLED, &call)
            .await,
        Err(Refusal::Conflict)
    );
    assert_eq!(assistant.written().len(), 1);
    assert_eq!(
        assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await,
        Ok(before)
    );
    assert_eq!(assistant.model_requests().len(), 2);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn deciding_a_call_that_isnt_awaiting_a_decision_is_a_conflict(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    let before = assistant.get(ALICE_CONVERSATION_ID_ROLES).await.unwrap();

    assert_eq!(
        assistant
            .approve(ALICE_CONVERSATION_ID_ROLES, LOOKUP_TOOL_CALL_ID_LIST_ROLES)
            .await,
        Err(Refusal::Conflict),
        "a read that ran"
    );
    assert_eq!(assistant.get(ALICE_CONVERSATION_ID_ROLES).await, Ok(before));
    assert!(
        assistant
            .announced::<ConversationToolCallDecided>()
            .await
            .is_empty()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn deciding_in_anothers_conversation_or_on_no_such_call_is_not_found(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    let call = awaiting_a_write(&assistant, json!({ "title": "Groceries" })).await;

    assert_eq!(
        assistant
            .approve(ALICE_CONVERSATION_ID_UNTITLED, uuid::Uuid::new_v4())
            .await,
        Err(Refusal::NotFound)
    );
    assistant.sign_in_as(BOB, &[]);
    assert_eq!(
        assistant
            .approve(ALICE_CONVERSATION_ID_UNTITLED, &call)
            .await,
        Err(Refusal::NotFound)
    );
    assert_eq!(assistant.written(), Vec::<Value>::new());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_turn_goes_on_once_every_write_of_its_step_is_decided(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.scripted([calls_all(&[
        ("create_list", json!({ "body": { "title": "Groceries" } })),
        ("create_list", json!({ "body": { "title": "Chores" } })),
    ])]);
    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Write two lists")
        .await
        .unwrap();
    let pending: Vec<String> = of_kind(&events, "approvalRequired")
        .iter()
        .map(|event| event["id"].as_str().unwrap().to_string())
        .collect();

    let first = assistant
        .approve(ALICE_CONVERSATION_ID_UNTITLED, &pending[0])
        .await
        .unwrap();
    assert_eq!(first.last().unwrap()["kind"], "finished");
    assert!(streamed_text(&first).is_empty());
    assert_eq!(assistant.model_requests().len(), 1, "one still awaits");

    assistant.scripted([says("One written, one not.")]);
    let second = assistant
        .reject(ALICE_CONVERSATION_ID_UNTITLED, &pending[1])
        .await
        .unwrap();
    assert_eq!(streamed_text(&second), "One written, one not.");
    assert_eq!(assistant.written(), vec![json!({ "title": "Groceries" })]);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn without_a_model_deciding_is_unavailable(pool: PgPool) {
    let assistant = Assistant::configured(pool, assistant::Settings::default).await;

    assert_eq!(
        assistant
            .approve(ALICE_CONVERSATION_ID_ROLES, LOOKUP_TOOL_CALL_ID_LIST_ROLES)
            .await,
        Err(Refusal::Unavailable)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_approval_the_app_refuses_as_signed_out_ends_the_turn_saying_so(pool: PgPool) {
    let assistant = Assistant::signed_out_of_the_app(pool).await;
    let call = awaiting_a_write(&assistant, json!({ "title": "Groceries" })).await;

    let events = assistant
        .approve(ALICE_CONVERSATION_ID_UNTITLED, &call)
        .await
        .expect("the resumed turn streams");

    assert_eq!(events[0]["kind"], "decided");
    let [result] = of_kind(&events, "toolResult").try_into().unwrap();
    assert_eq!(result["error"]["code"], "unauthorized");
    let [error] = of_kind(&events, "error").try_into().unwrap();
    assert_eq!(error["error"]["code"], "unauthorized");
    assert_eq!(
        assistant.model_requests().len(),
        1,
        "the model isn't asked again"
    );
    let conversation = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    let messages = conversation["messages"].as_array().unwrap();
    assert!(
        messages.last().unwrap()["text"]
            .as_str()
            .unwrap()
            .contains("sign-in expired")
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn approving_two_writes_of_a_step_at_once_runs_each_once(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.scripted([
        calls_all(&[
            ("create_list", json!({ "body": { "title": "Groceries" } })),
            ("create_list", json!({ "body": { "title": "Chores" } })),
        ]),
        says("Both written."),
    ]);
    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Write two lists")
        .await
        .unwrap();
    let pending: Vec<String> = of_kind(&events, "approvalRequired")
        .iter()
        .map(|event| event["id"].as_str().unwrap().to_string())
        .collect();

    let (first, second) = tokio::join!(
        assistant.approve(ALICE_CONVERSATION_ID_UNTITLED, &pending[0]),
        assistant.approve(ALICE_CONVERSATION_ID_UNTITLED, &pending[1]),
    );

    assert!(
        first.is_ok() && second.is_ok(),
        "the save that lost is retried"
    );
    let mut written = assistant.written_with_keys();
    written.sort_by_key(|(_, key)| key.clone());
    let mut expected = vec![
        (
            json!({ "title": "Groceries" }),
            Some(format!("assistant-tool-call-{}", pending[0])),
        ),
        (
            json!({ "title": "Chores" }),
            Some(format!("assistant-tool-call-{}", pending[1])),
        ),
    ];
    expected.sort_by_key(|(_, key)| key.clone());
    assert_eq!(written, expected, "each write once, a retry replayed");
    let conversation = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    for call in &pending {
        assert_eq!(status_of(&conversation, call)["status"], "approved");
    }
    assert_eq!(assistant.model_requests().len(), 2, "the turn goes on once");
}
