use crate::driver::{Notes, Refusal};
use notes::seed::{ALICES_NOTE, BOBS_NOTE, ORPHANED_NOTE};
use sqlx::PgPool;

#[sqlx::test(migrations = false)]
async fn notes_are_listed_most_recent_first(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let page = notes.list().await;
    assert_eq!(page.total, 3);
    assert_eq!(
        page.ids(),
        [
            ORPHANED_NOTE.to_string(),
            BOBS_NOTE.to_string(),
            ALICES_NOTE.to_string(),
        ]
    );
}

#[sqlx::test(migrations = false)]
async fn notes_can_be_sorted_by_title(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let page = notes
        .list_sorted(&["title"])
        .await
        .expect("a known sort key");
    assert_eq!(
        page.ids(),
        [
            BOBS_NOTE.to_string(),
            ORPHANED_NOTE.to_string(),
            ALICES_NOTE.to_string(),
        ]
    );
}

#[sqlx::test(migrations = false)]
async fn notes_can_be_sorted_descending(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let page = notes
        .list_sorted(&["-title"])
        .await
        .expect("a known sort key");
    assert_eq!(
        page.ids(),
        [
            ALICES_NOTE.to_string(),
            ORPHANED_NOTE.to_string(),
            BOBS_NOTE.to_string(),
        ]
    );
}

#[sqlx::test(migrations = false)]
async fn notes_can_be_sorted_oldest_first(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let page = notes
        .list_sorted(&["createdAt"])
        .await
        .expect("a known sort key");
    assert_eq!(
        page.ids(),
        [
            ALICES_NOTE.to_string(),
            BOBS_NOTE.to_string(),
            ORPHANED_NOTE.to_string(),
        ]
    );
}

#[sqlx::test(migrations = false)]
async fn sorting_by_an_unknown_key_is_malformed(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.list_sorted(&["body"]).await.err(),
        Some(Refusal::Malformed)
    );
}

#[sqlx::test(migrations = false)]
async fn a_page_holds_its_slice_and_counts_every_note(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let page = notes.list_page(1, 1).await;
    assert_eq!(page.ids(), [BOBS_NOTE.to_string()]);
    assert_eq!((page.total, page.limit, page.offset), (3, 1, 1));
}

#[sqlx::test(migrations = false)]
async fn an_oversized_page_is_clamped(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(notes.list_page(10_000, 0).await.limit, 100);
}
