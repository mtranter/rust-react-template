use crate::driver::{Assistant, LIST_ID, calls, of_kind};
use assistant::seed::ALICE_CONVERSATION_ID_UNTITLED;
use serde_json::{Value, json};
use sqlx::PgPool;

const ANOTHERS_LIST_ID: &str = "22222222-2222-2222-2222-222222222222";
const OWNER_ID: &str = "33333333-3333-3333-3333-333333333333";

/// Has the model ask for `tool` with `arguments` in Alice's untitled conversation: the pending
/// call's proposal as the stream told it, and as the conversation tells it when reopened.
async fn proposed(assistant: &Assistant, tool: &str, arguments: Value) -> (Value, Value) {
    assistant.scripted([calls(tool, arguments)]);
    let events = assistant
        .send(ALICE_CONVERSATION_ID_UNTITLED, 1, "Please do it")
        .await
        .unwrap();
    let pending = of_kind(&events, "approvalRequired")[0];
    let reopened = assistant.get(ALICE_CONVERSATION_ID_UNTITLED).await.unwrap();
    let stored = reopened["messages"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|message| message["toolCalls"].as_array().cloned().unwrap_or_default())
        .find(|call| call["id"] == pending["id"])
        .unwrap();
    (pending["proposal"].clone(), stored["proposal"].clone())
}

fn edit(list: &str, title: &str) -> Value {
    json!({
        "id": list,
        "expected_version": "4",
        "idempotency_key": "the-models-own-key",
        "body": { "title": title, "dueOn": "2026-12-24", "ownerId": OWNER_ID },
    })
}

/// Each label and value a proposal shows.
fn shown(proposal: &Value) -> Vec<(String, Value)> {
    proposal["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| {
            (
                field["label"].as_str().unwrap().to_owned(),
                field["value"].clone(),
            )
        })
        .collect()
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_pending_edit_names_its_action_and_target_and_shows_only_business_data(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    let (streamed, reopened) = proposed(&assistant, "edit_list", edit(LIST_ID, "Groceries")).await;

    assert_eq!(
        streamed, reopened,
        "the same proposal streamed and reopened"
    );
    assert_eq!(streamed["action"], "Edit list");
    assert_eq!(
        streamed["target"],
        json!({ "kind": "List", "name": "Shopping" })
    );
    let labels: Vec<&str> = streamed["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| field["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["Title", "Due on"], "in the schema's order");
    assert_eq!(
        streamed["fields"][1],
        json!({ "label": "Due on", "value": "2026-12-24", "format": "date" })
    );
    for (label, value) in shown(&streamed) {
        for leaked in ["4", "the-models-own-key", OWNER_ID, LIST_ID] {
            assert_ne!(value, leaked, "{label} shows {leaked}");
        }
        let label = label.to_lowercase();
        for leaked in ["version", "key", "id"] {
            assert!(
                !label.split(' ').any(|word| word == leaked),
                "{label} is shown"
            );
        }
    }
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_field_the_call_changes_carries_its_previous_value(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    let (proposal, _) = proposed(&assistant, "edit_list", edit(LIST_ID, "Groceries")).await;

    assert_eq!(
        proposal["fields"][0],
        json!({ "label": "Title", "value": "Groceries", "previous": "Shopping" })
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_field_the_call_leaves_as_it_is_has_no_previous_value(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    let (proposal, _) = proposed(&assistant, "edit_list", edit(LIST_ID, "Shopping")).await;

    assert_eq!(
        proposal["fields"][0],
        json!({ "label": "Title", "value": "Shopping" })
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_call_whose_target_cant_be_read_has_no_target_and_still_shows_its_fields(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    let (streamed, reopened) =
        proposed(&assistant, "edit_list", edit(ANOTHERS_LIST_ID, "Groceries")).await;

    assert_eq!(streamed, reopened);
    assert_eq!(streamed["action"], "Edit list");
    assert!(streamed.get("target").is_none(), "no target: {streamed}");
    assert_eq!(
        streamed["fields"][0],
        json!({ "label": "Title", "value": "Groceries" }),
        "nothing read, so nothing previous"
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_pending_delete_names_its_target(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    let (streamed, reopened) = proposed(
        &assistant,
        "delete_list",
        json!({ "id": LIST_ID, "expected_version": "4" }),
    )
    .await;

    assert_eq!(streamed, reopened);
    assert_eq!(
        streamed,
        json!({
            "action": "Delete list",
            "target": { "kind": "List", "name": "Shopping" },
            "fields": [],
        })
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_write_with_no_record_to_act_on_has_no_target(pool: PgPool) {
    let assistant = Assistant::on(pool).await;

    let (proposal, _) = proposed(
        &assistant,
        "create_list",
        json!({ "body": { "title": "Chores" } }),
    )
    .await;

    assert_eq!(
        proposal,
        json!({
            "action": "Create list",
            "fields": [{ "label": "Title", "value": "Chores" }],
        })
    );
}
