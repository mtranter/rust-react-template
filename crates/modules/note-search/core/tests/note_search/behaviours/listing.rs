use crate::driver::{NoteSearch, NoteSummaryJson, Refusal};
use notes_api::seed::{self, ALICES_NOTE, BOBS_NOTE, ORPHANED_NOTE};
use sqlx::PgPool;

#[sqlx::test(migrations = false)]
async fn every_note_is_listed_most_recent_first(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    let found = search.all().await;
    assert_eq!(found.total, 3);
    assert_eq!(
        found.ids(),
        [
            ORPHANED_NOTE.to_string(),
            BOBS_NOTE.to_string(),
            ALICES_NOTE.to_string(),
        ]
    );
}

#[sqlx::test(migrations = false)]
async fn a_note_is_listed_with_its_authors_name(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    assert_eq!(
        search.all().await.item(ALICES_NOTE),
        &NoteSummaryJson::of(seed::alices_note(), Some(users_api::seed::alice()))
    );
}

#[sqlx::test(migrations = false)]
async fn a_note_whose_author_is_not_a_user_is_listed_without_an_author_name(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    assert_eq!(
        search.all().await.item(ORPHANED_NOTE),
        &NoteSummaryJson::of(seed::orphaned_note(), None)
    );
}

#[sqlx::test(migrations = false)]
async fn notes_can_be_sorted_by_author_name_with_unknown_authors_last(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    let found = search
        .sorted(&["authorName"])
        .await
        .expect("a known sort key");
    assert_eq!(
        found.ids(),
        [
            ALICES_NOTE.to_string(),
            BOBS_NOTE.to_string(),
            ORPHANED_NOTE.to_string(),
        ]
    );
}

#[sqlx::test(migrations = false)]
async fn notes_can_be_sorted_by_title_descending(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    let found = search.sorted(&["-title"]).await.expect("a known sort key");
    assert_eq!(
        found.ids(),
        [
            ALICES_NOTE.to_string(),
            ORPHANED_NOTE.to_string(),
            BOBS_NOTE.to_string(),
        ]
    );
}

#[sqlx::test(migrations = false)]
async fn sorting_by_an_unknown_key_is_malformed(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    assert_eq!(
        search.sorted(&["body"]).await.err(),
        Some(Refusal::Malformed)
    );
}
