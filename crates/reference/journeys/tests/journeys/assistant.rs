use crate::driver::App;
use notes_api::seed::ALICE_NOTE_ID_SHOPPING_LIST;
use serde_json::{Value, json};
use sqlx::PgPool;
use users_api::seed::{USER_ID_ALICE, USER_ID_BOB};

/// What the stand-in model is told to ask for: Alice's shopping list.
fn read_alices_note() -> String {
    json!({
        "tool": "get_note",
        "arguments": { "id": ALICE_NOTE_ID_SHOPPING_LIST.to_string() },
    })
    .to_string()
}

fn tool_results(events: &[Value]) -> Vec<&Value> {
    events
        .iter()
        .filter(|event| event["kind"] == "toolResult")
        .collect()
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_assistant_reads_what_its_user_may_and_not_what_they_may_not(pool: PgPool) {
    let app = App::seeded(pool).await;
    let alice = app.as_user(USER_ID_ALICE);
    let bob = app.as_user(USER_ID_BOB);

    let alices = alice.start_conversation().await.unwrap();
    let events = alice
        .ask_assistant(&alices, &read_alices_note())
        .await
        .unwrap();
    let [read] = tool_results(&events).try_into().unwrap();
    assert_eq!(read["result"]["title"], "Shopping list");

    let bobs = bob.start_conversation().await.unwrap();
    let events = bob.ask_assistant(&bobs, &read_alices_note()).await.unwrap();
    let [refused] = tool_results(&events).try_into().unwrap();
    assert_eq!(refused["error"]["code"], "not_found");
    assert_eq!(events.last().unwrap()["kind"], "finished");

    let stored = bob
        .conversation(bobs["id"].as_str().unwrap())
        .await
        .unwrap();
    let call = &stored["messages"][1]["toolCalls"][0];
    assert_eq!(call["tool"], "get_note");
    assert_eq!(call["status"], "ran");
    assert_eq!(call["error"]["code"], "not_found");
    assert!(
        !stored.to_string().contains("Shopping list"),
        "nothing of Alice's note reaches Bob's conversation"
    );
}
