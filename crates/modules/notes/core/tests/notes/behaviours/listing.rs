use crate::driver::{Notes, Refusal};
use notes::seed::{ALICE_NOTE_ID_SHOPPING_LIST, BOB_NOTE_ID_BIKE_REPAIRS, NOTE_ID_ORPHANED};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn notes_are_listed_most_recent_first(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let page = notes.list().await;
    assert_eq!(page.total, 3);
    assert_eq!(
        page.ids(),
        [
            NOTE_ID_ORPHANED.to_string(),
            BOB_NOTE_ID_BIKE_REPAIRS.to_string(),
            ALICE_NOTE_ID_SHOPPING_LIST.to_string(),
        ]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn notes_can_be_sorted_by_title(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let page = notes
        .list_sorted(&["title"])
        .await
        .expect("a known sort key");
    assert_eq!(
        page.ids(),
        [
            BOB_NOTE_ID_BIKE_REPAIRS.to_string(),
            NOTE_ID_ORPHANED.to_string(),
            ALICE_NOTE_ID_SHOPPING_LIST.to_string(),
        ]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn notes_can_be_sorted_descending(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let page = notes
        .list_sorted(&["-title"])
        .await
        .expect("a known sort key");
    assert_eq!(
        page.ids(),
        [
            ALICE_NOTE_ID_SHOPPING_LIST.to_string(),
            NOTE_ID_ORPHANED.to_string(),
            BOB_NOTE_ID_BIKE_REPAIRS.to_string(),
        ]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn notes_can_be_sorted_oldest_first(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let page = notes
        .list_sorted(&["createdAt"])
        .await
        .expect("a known sort key");
    assert_eq!(
        page.ids(),
        [
            ALICE_NOTE_ID_SHOPPING_LIST.to_string(),
            BOB_NOTE_ID_BIKE_REPAIRS.to_string(),
            NOTE_ID_ORPHANED.to_string(),
        ]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn sorting_by_an_unknown_key_is_malformed(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.list_sorted(&["body"]).await.err(),
        Some(Refusal::Malformed)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_page_holds_its_slice_and_counts_every_note(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let page = notes.list_page(1, 1).await;
    assert_eq!(page.ids(), [BOB_NOTE_ID_BIKE_REPAIRS.to_string()]);
    assert_eq!((page.total, page.limit, page.offset), (3, 1, 1));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_oversized_page_is_clamped(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(notes.list_page(10_000, 0).await.limit, 100);
}
