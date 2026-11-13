use crate::driver::{NoteSearch, NoteSummaryJson, Refusal};
use notes_api::seed::{
    self, ALICE_NOTE_ID_SHOPPING_LIST, BOB_NOTE_ID_BIKE_REPAIRS, NOTE_ID_ORPHANED,
};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn every_note_is_listed_most_recent_first(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    let found = search.all().await;
    assert_eq!(found.total, 3);
    assert_eq!(
        found.ids(),
        [
            NOTE_ID_ORPHANED.to_string(),
            BOB_NOTE_ID_BIKE_REPAIRS.to_string(),
            ALICE_NOTE_ID_SHOPPING_LIST.to_string(),
        ]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_note_is_listed_with_its_authors_name(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    assert_eq!(
        search.all().await.item(ALICE_NOTE_ID_SHOPPING_LIST),
        &NoteSummaryJson::of(seed::alices_note(), Some(users_api::seed::alice()))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_note_whose_author_is_not_a_user_is_listed_without_an_author_name(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    assert_eq!(
        search.all().await.item(NOTE_ID_ORPHANED),
        &NoteSummaryJson::of(seed::orphaned_note(), None)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn notes_can_be_sorted_by_author_name_with_unknown_authors_last(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    let found = search
        .sorted(&["authorName"])
        .await
        .expect("a known sort key");
    assert_eq!(
        found.ids(),
        [
            ALICE_NOTE_ID_SHOPPING_LIST.to_string(),
            BOB_NOTE_ID_BIKE_REPAIRS.to_string(),
            NOTE_ID_ORPHANED.to_string(),
        ]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn notes_can_be_sorted_by_title_descending(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    let found = search.sorted(&["-title"]).await.expect("a known sort key");
    assert_eq!(
        found.ids(),
        [
            ALICE_NOTE_ID_SHOPPING_LIST.to_string(),
            NOTE_ID_ORPHANED.to_string(),
            BOB_NOTE_ID_BIKE_REPAIRS.to_string(),
        ]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn sorting_by_an_unknown_key_is_malformed(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    assert_eq!(
        search.sorted(&["body"]).await.err(),
        Some(Refusal::Malformed)
    );
}
