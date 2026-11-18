use crate::driver::{Notes, Refusal, USER};
use notes::seed::{ALICE_NOTE_ID_SHOPPING_LIST, BOB_NOTE_ID_BIKE_REPAIRS, NOTE_ID_ORPHANED};
use sqlx::PgPool;
use users_api::seed::{USER_ID_ALICE, USER_ID_BOB};

/// An admin sees every note, so these list the seeds' three.
async fn as_admin(pool: PgPool) -> Notes {
    let notes = Notes::seeded(pool).await;
    notes.sign_in_as(USER_ID_BOB, &["admin"]);
    notes
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_lists_only_their_own_notes(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let page = notes.list().await;
    assert_eq!(page.total, 1);
    assert_eq!(page.ids(), [ALICE_NOTE_ID_SHOPPING_LIST.to_string()]);

    notes.sign_in_as(USER_ID_BOB, &[USER]);
    assert_eq!(
        notes.list().await.ids(),
        [BOB_NOTE_ID_BIKE_REPAIRS.to_string()]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_users_pages_and_total_count_only_their_own_notes(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let newer = notes.create(USER_ID_ALICE, "Newer", "").await.unwrap();
    notes.create(USER_ID_BOB, "Bob's newer", "").await.unwrap();

    let page = notes.list_page(1, 0).await;
    assert_eq!(page.ids(), [newer.id().to_string()]);
    assert_eq!(page.total, 2);
    let page = notes.list_page(1, 1).await;
    assert_eq!(page.ids(), [ALICE_NOTE_ID_SHOPPING_LIST.to_string()]);
    assert_eq!(page.total, 2);
    assert!(notes.list_page(1, 2).await.ids().is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn notes_are_listed_most_recent_first(pool: PgPool) {
    let notes = as_admin(pool).await;
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
    let notes = as_admin(pool).await;
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
    let notes = as_admin(pool).await;
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
    let notes = as_admin(pool).await;
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
    let notes = as_admin(pool).await;
    let page = notes.list_page(1, 1).await;
    assert_eq!(page.ids(), [BOB_NOTE_ID_BIKE_REPAIRS.to_string()]);
    assert_eq!((page.total, page.limit, page.offset), (3, 1, 1));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_oversized_page_is_clamped(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(notes.list_page(10_000, 0).await.limit, 100);
}
