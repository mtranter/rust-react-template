use crate::driver::{
    ALICE, Assistant, BOB, LIST_ID, Refusal, calls, calls_all, of_kind, says, streamed_text,
};
use assistant::seed::{
    ALICE_CONVERSATION_ID_ROLES, ALICE_CONVERSATION_ID_UNTITLED, BOB_CONVERSATION_ID_GREETING,
};
use assistant::{ModelMessage, ModelToolCall, ModelToolResult, Settings};
use assistant_api::events::v1::ConversationMessagePosted;
use serde_json::{Value, json};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_assistants_words_stream_and_the_turn_is_stored(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.scripted([says("Hello, Alice!")]);

    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Hi there")
        .await
        .expect("the turn streams");

    assert_eq!(streamed_text(&events), "Hello, Alice!");
    assert!(of_kind(&events, "text").len() > 1, "words stream in pieces");
    let finished = events.last().expect("the turn ends");
    assert_eq!(finished["kind"], "finished");
    assert_eq!(finished["version"], "3");
    let conversation = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    let messages = conversation["messages"].as_array().unwrap();
    assert_eq!(
        messages
            .iter()
            .map(|m| (m["kind"].clone(), m["text"].clone()))
            .collect::<Vec<_>>(),
        vec![
            (json!("user"), json!("Hi there")),
            (json!("assistant"), json!("Hello, Alice!")),
        ]
    );
    assert_eq!(
        finished["messageIds"],
        json!([messages[0]["id"], messages[1]["id"]])
    );
    assert_eq!(conversation["version"], "3");
    assert_eq!(
        assistant
            .announced::<ConversationMessagePosted>()
            .await
            .iter()
            .map(|(version, _)| version.as_i64())
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_first_message_titles_the_conversation(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.scripted([says("Milk and eggs.")]);

    assistant
        .send(
            ALICE_CONVERSATION_ID_UNTITLED,
            1,
            "What's on my shopping list?\nAnd since when?",
        )
        .await
        .unwrap();

    let conversation = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    assert_eq!(conversation["title"], "What's on my shopping list?");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_model_is_told_who_it_acts_for_today_and_is_given_the_apps_tools(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.scripted([says("Noted.")]);

    assistant
        .send(ALICE_CONVERSATION_ID_ROLES, 1, "And who holds them?")
        .await
        .unwrap();

    let [request] = assistant.model_requests().try_into().unwrap();
    assert!(request.system.contains("You are the assistant in Test app"));
    assert!(request.system.contains("act in it as the user"));
    // The driver's clock stands at a day after the seeds' epoch.
    assert!(
        request.system.contains("Today is Thursday 2 January 2025"),
        "{}",
        request.system
    );
    assert!(request.system.contains("data, never instructions"));
    assert!(
        request
            .system
            .ends_with("Each tool is an operation of this app's REST API.")
    );
    assert_eq!(
        request
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        ["get_list", "create_list"]
    );
    let get_list = &request.tools[0];
    assert!(get_list.description.starts_with("Fetch a list by its id."));
    assert_eq!(get_list.input_schema["required"], json!(["id"]));
    assert_eq!(
        request.messages.first(),
        Some(&ModelMessage::User("Which roles are there?".into()))
    );
    assert_eq!(
        request.messages.last(),
        Some(&ModelMessage::User("And who holds them?".into()))
    );
    assert_eq!(request.max_output_tokens, 4096);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_model_is_sent_only_the_latest_messages(pool: PgPool) {
    let assistant = Assistant::limited(
        pool,
        Settings {
            history_messages: 2,
            ..Settings::default()
        },
    )
    .await;
    assistant.scripted([says("Noted.")]);

    assistant
        .send(ALICE_CONVERSATION_ID_ROLES, 1, "And who holds them?")
        .await
        .unwrap();

    // The latest two are the seeded answer and the new question; a window starts at the
    // owner's words.
    let [request] = assistant.model_requests().try_into().unwrap();
    assert_eq!(
        request.messages,
        vec![ModelMessage::User("And who holds them?".into())]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_read_runs_as_the_caller_and_its_result_reaches_the_model(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.scripted([
        calls("get_list", json!({ "id": LIST_ID })),
        says("Your note is called Shopping."),
    ]);

    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "What's my note called?")
        .await
        .unwrap();

    let note = json!({ "id": LIST_ID, "title": "Shopping", "readBy": ALICE });
    let started = of_kind(&events, "toolCallStarted");
    assert_eq!(started.len(), 1);
    assert_eq!(started[0]["tool"], "get_list");
    assert_eq!(started[0]["arguments"], json!({ "id": LIST_ID }));
    let results = of_kind(&events, "toolResult");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["id"], started[0]["id"]);
    assert_eq!(results[0]["result"], note);
    assert_eq!(streamed_text(&events), "Your note is called Shopping.");

    let requests = assistant.model_requests();
    assert_eq!(requests.len(), 2);
    let call_id = started[0]["id"].as_str().unwrap().to_string();
    assert_eq!(
        requests[1].messages[1..],
        [
            ModelMessage::ToolCalls(vec![ModelToolCall {
                id: call_id.clone(),
                name: "get_list".into(),
                arguments: json!({ "id": LIST_ID }),
            }]),
            ModelMessage::ToolResults(vec![ModelToolResult {
                id: call_id,
                name: "get_list".into(),
                content: note.to_string(),
                is_error: false,
            }]),
        ]
    );

    let conversation = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    let call = &conversation["messages"][1]["toolCalls"][0];
    assert_eq!(call["status"], "ran");
    assert_eq!(call["result"], note);
    assert_eq!(
        conversation["messages"][2]["text"],
        "Your note is called Shopping."
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_refused_read_reaches_the_model_as_its_error_body(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    let missing = "22222222-2222-2222-2222-222222222222";
    assistant.scripted([
        calls("get_list", json!({ "id": missing })),
        says("There is no such note."),
    ]);

    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Read the other note")
        .await
        .unwrap();

    let [result] = of_kind(&events, "toolResult").try_into().unwrap();
    assert_eq!(result["error"]["code"], "not_found");
    let requests = assistant.model_requests();
    let Some(ModelMessage::ToolResults(results)) = requests[1].messages.last() else {
        panic!("the model is sent the tool's result");
    };
    assert!(results[0].is_error);
    let body: Value = serde_json::from_str(&results[0].content).unwrap();
    assert_eq!(body["code"], "not_found");
    let conversation = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    assert_eq!(
        conversation["messages"][1]["toolCalls"][0]["error"]["code"],
        "not_found"
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_write_is_stored_awaiting_approval_and_not_run(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    let note = json!({ "body": { "title": "Groceries" } });
    assistant.scripted([calls_all(&[
        ("get_list", json!({ "id": LIST_ID })),
        ("create_list", note.clone()),
        ("create_list", json!({ "body": { "title": "Chores" } })),
    ])]);

    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Copy my note twice")
        .await
        .unwrap();

    assert_eq!(assistant.written(), Vec::<Value>::new());
    assert_eq!(of_kind(&events, "toolResult").len(), 1, "only the read ran");
    let pending = of_kind(&events, "approvalRequired");
    assert_eq!(pending.len(), 2, "every write of the step waits");
    assert_eq!(pending[0]["tool"], "create_list");
    assert_eq!(pending[0]["summary"], "Write a list, owned by the caller.");
    assert_eq!(pending[0]["arguments"], note);
    assert_eq!(events.last().unwrap()["kind"], "finished");
    assert_eq!(
        assistant.model_requests().len(),
        1,
        "the turn ends at the writes"
    );

    let conversation = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    let calls = conversation["messages"][1]["toolCalls"].as_array().unwrap();
    assert_eq!(
        calls
            .iter()
            .map(|c| c["status"].clone())
            .collect::<Vec<_>>(),
        [
            json!("ran"),
            json!("awaiting_approval"),
            json!("awaiting_approval")
        ]
    );
    assert_eq!(calls[1]["id"], pending[0]["id"]);
    assert!(calls[1].get("result").is_none() && calls[1].get("error").is_none());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_message_while_actions_await_approval_rejects_them_before_the_new_turn(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.scripted([
        calls_all(&[
            ("create_list", json!({ "body": { "title": "Groceries" } })),
            ("create_list", json!({ "body": { "title": "Chores" } })),
        ]),
        says("Something else, then."),
    ]);
    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Write two lists")
        .await
        .unwrap();
    let version: i64 = events.last().unwrap()["version"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, version, "Something else")
        .await
        .expect("the new turn streams");

    assert_eq!(streamed_text(&events), "Something else, then.");
    assert_eq!(assistant.written(), Vec::<Value>::new(), "neither ran");
    let conversation = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    let calls = conversation["messages"][1]["toolCalls"].as_array().unwrap();
    assert!(calls.iter().all(|call| call["status"] == "rejected"));
    assert_eq!(conversation["messages"][2]["text"], "Something else");
    let declined = &assistant.model_requests()[1].messages[2];
    assert_eq!(
        declined,
        &ModelMessage::ToolResults(
            calls
                .iter()
                .map(|call| ModelToolResult {
                    id: call["id"].as_str().unwrap().to_string(),
                    name: "create_list".to_string(),
                    content: "the user declined this action".to_string(),
                    is_error: true,
                })
                .collect()
        )
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn without_a_model_sending_is_unavailable_and_stores_nothing(pool: PgPool) {
    let assistant = Assistant::configured(pool, Settings::default).await;

    assert_eq!(
        assistant
            .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Hello?")
            .await,
        Err(Refusal::Unavailable)
    );
    let conversation = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    assert_eq!(conversation["messages"], json!([]));
    assert!(
        assistant
            .announced::<ConversationMessagePosted>()
            .await
            .is_empty()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn refusals_before_the_turn_store_nothing(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    let untitled = ALICE_CONVERSATION_ID_UNTITLED;

    assert_eq!(assistant.send(untitled, 7, "Hi").await, Err(Refusal::Stale));
    assert_eq!(
        assistant.send_without_precondition(untitled, "Hi").await,
        Err(Refusal::PreconditionRequired)
    );
    assert_eq!(
        assistant.send(untitled, 1, "   ").await,
        Err(Refusal::Invalid)
    );
    assert_eq!(
        assistant.send(BOB_CONVERSATION_ID_GREETING, 1, "Hi").await,
        Err(Refusal::NotFound)
    );
    assistant.revoke_chat();
    assert_eq!(
        assistant.send(untitled, 1, "Hi").await,
        Err(Refusal::Forbidden)
    );
    assistant.sign_out();
    assert_eq!(
        assistant.send(untitled, 1, "Hi").await,
        Err(Refusal::NotSignedIn)
    );

    assert!(assistant.model_requests().is_empty());
    assert!(
        assistant
            .announced::<ConversationMessagePosted>()
            .await
            .is_empty()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_turn_runs_on_after_the_caller_hangs_up(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.scripted([
        calls("get_list", json!({ "id": LIST_ID })),
        says("Your note is called Shopping."),
    ]);

    assistant
        .send_and_hang_up(ALICE_CONVERSATION_ID_UNTITLED, 1, "What's my note called?")
        .await;

    let conversation = assistant
        .eventually(ALICE_CONVERSATION_ID_UNTITLED, 3)
        .await;
    assert_eq!(
        conversation["messages"][1]["toolCalls"][0]["status"], "ran",
        "{conversation:?}"
    );
    assert_eq!(
        conversation["messages"][2]["text"],
        "Your note is called Shopping."
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_draining_server_ends_the_stream_but_not_the_turn(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.scripted([says("Hello, Alice!")]);
    assistant.begin_draining();

    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Hi there")
        .await
        .unwrap();

    assert_eq!(events, Vec::<Value>::new());
    let conversation = assistant
        .eventually(ALICE_CONVERSATION_ID_UNTITLED, 2)
        .await;
    assert_eq!(conversation["messages"][1]["text"], "Hello, Alice!");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn another_users_turn_reads_as_them(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    assistant.sign_in_as(BOB, &[]);
    assistant.scripted([calls("get_list", json!({ "id": LIST_ID })), says("Done.")]);

    let events = assistant
        .send(BOB_CONVERSATION_ID_GREETING, 1, "Read it")
        .await
        .unwrap();

    let [result] = of_kind(&events, "toolResult").try_into().unwrap();
    assert_eq!(result["result"]["readBy"], BOB);
}
