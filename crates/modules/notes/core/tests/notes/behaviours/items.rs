use crate::driver::{NoteJson, Notes, Refusal, announced};
use app_events::EventPayload;
use app_persistence::Version;
use notes::NoteId;
use notes::seed::{
    self, ALICE_NOTE_ID_SHOPPING_LIST, BOB_NOTE_ID_BIKE_REPAIRS, SHOPPING_LIST_ITEM_ID_EGGS,
    SHOPPING_LIST_ITEM_ID_MILK,
};
use notes_api::events::v1::{self, NoteItemAdded, NoteItemRemoved};
use serde_json::json;
use sqlx::PgPool;
use users_api::seed::USER_ID_ALICE;

fn texts(note: &NoteJson) -> Vec<&str> {
    note["items"]
        .as_array()
        .expect("a note lists its items")
        .iter()
        .map(|item| item["text"].as_str().expect("an item has text"))
        .collect()
}

fn item_id(note: &NoteJson, text: &str) -> String {
    note["items"]
        .as_array()
        .expect("a note lists its items")
        .iter()
        .find(|item| item["text"] == text)
        .and_then(|item| item["id"].as_str())
        .expect("the note has the item")
        .to_owned()
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_added_item_goes_last_on_the_note(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let added = notes
        .add_item(ALICE_NOTE_ID_SHOPPING_LIST, 1, " Bread ")
        .await
        .unwrap();

    assert_eq!(texts(&added), ["Milk", "Eggs", "Bread"]);
    assert_eq!(added.version(), 2);
    assert_eq!(notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await, Ok(added));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn adding_an_item_announces_the_note_and_names_the_item(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let added = notes
        .add_item(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Bread")
        .await
        .unwrap();

    let bread = item_id(&added, "Bread").parse().unwrap();
    let mut note = announced(&seed::alices_note()).entity;
    note.items.push(v1::Item {
        id: bread,
        text: "Bread".into(),
    });
    assert_eq!(
        notes.announced::<NoteItemAdded>().await,
        [(
            Version::from_i64(2),
            EventPayload {
                entity: note,
                context: v1::OfItem { item_id: bread },
            }
        )]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_item_without_text_is_rejected_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    assert_eq!(
        notes.add_item(ALICE_NOTE_ID_SHOPPING_LIST, 1, "  ").await,
        Err(Refusal::invalid(["text"]))
    );
    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(seed::alices_note().into())
    );
    assert!(notes.announced::<NoteItemAdded>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_item_already_on_the_note_is_rejected_whatever_its_case(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    assert_eq!(
        notes.add_item(ALICE_NOTE_ID_SHOPPING_LIST, 1, "MILK").await,
        Err(Refusal::invalid(["text"]))
    );
    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(seed::alices_note().into())
    );
    assert!(notes.announced::<NoteItemAdded>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_same_item_may_be_on_two_notes(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let added = notes
        .add_item(BOB_NOTE_ID_BIKE_REPAIRS, 1, "Milk")
        .await
        .unwrap();

    assert_eq!(texts(&added), ["Milk"]);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn removing_an_item_leaves_the_rest_in_order_and_names_it(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let removed = notes
        .remove_item(ALICE_NOTE_ID_SHOPPING_LIST, 1, SHOPPING_LIST_ITEM_ID_MILK)
        .await
        .unwrap();

    assert_eq!(texts(&removed), ["Eggs"]);
    assert_eq!(notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await, Ok(removed));
    let mut note = announced(&seed::alices_note()).entity;
    note.items
        .retain(|item| item.id == SHOPPING_LIST_ITEM_ID_EGGS.as_uuid());
    assert_eq!(
        notes.announced::<NoteItemRemoved>().await,
        [(
            Version::from_i64(2),
            EventPayload {
                entity: note,
                context: v1::OfItem {
                    item_id: SHOPPING_LIST_ITEM_ID_MILK.as_uuid(),
                },
            }
        )]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn removing_an_item_the_note_does_not_have_is_not_found(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    assert_eq!(
        notes
            .remove_item(BOB_NOTE_ID_BIKE_REPAIRS, 1, SHOPPING_LIST_ITEM_ID_MILK)
            .await,
        Err(Refusal::NotFound)
    );
    assert!(notes.announced::<NoteItemRemoved>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_item_on_a_missing_note_is_not_found(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    assert_eq!(
        notes.add_item(NoteId::new(), 1, "Bread").await,
        Err(Refusal::NotFound)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_item_change_against_an_edited_note_is_stale_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let edited = notes
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Weekend shopping", "")
        .await
        .unwrap();

    assert_eq!(
        notes
            .add_item(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Bread")
            .await,
        Err(Refusal::Stale)
    );
    assert_eq!(
        notes
            .remove_item(ALICE_NOTE_ID_SHOPPING_LIST, 1, SHOPPING_LIST_ITEM_ID_MILK)
            .await,
        Err(Refusal::Stale)
    );
    assert_eq!(notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await, Ok(edited));
    assert!(notes.announced::<NoteItemAdded>().await.is_empty());
    assert!(notes.announced::<NoteItemRemoved>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_item_change_makes_an_edit_from_before_it_stale(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let added = notes
        .add_item(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Bread")
        .await
        .unwrap();

    assert_eq!(
        notes
            .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Weekend shopping", "")
            .await,
        Err(Refusal::Stale)
    );
    assert_eq!(notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await, Ok(added));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_archived_notes_items_are_read_only(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    notes.user_deleted(USER_ID_ALICE).await;
    notes.run_pending_events().await;
    let archived = notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await.unwrap();

    assert_eq!(
        notes
            .add_item(ALICE_NOTE_ID_SHOPPING_LIST, 2, "Bread")
            .await,
        Err(Refusal::Conflict)
    );
    assert_eq!(
        notes
            .remove_item(ALICE_NOTE_ID_SHOPPING_LIST, 2, SHOPPING_LIST_ITEM_ID_MILK)
            .await,
        Err(Refusal::Conflict)
    );
    assert_eq!(notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await, Ok(archived));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn editing_a_note_keeps_its_items(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let edited = notes
        .edit(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Weekend shopping", "")
        .await
        .unwrap();

    assert_eq!(
        edited["items"],
        json!([
            { "id": SHOPPING_LIST_ITEM_ID_MILK.to_string(), "text": "Milk" },
            { "id": SHOPPING_LIST_ITEM_ID_EGGS.to_string(), "text": "Eggs" },
        ])
    );
}
