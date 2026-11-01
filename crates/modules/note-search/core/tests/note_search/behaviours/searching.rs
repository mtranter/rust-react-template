use crate::driver::NoteSearch;
use notes_api::seed::{ALICES_NOTE, BOBS_NOTE};
use sqlx::PgPool;

#[sqlx::test(migrations = false)]
async fn notes_can_be_found_by_their_authors_name(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    assert_eq!(
        search.matching("alice").await.ids(),
        [ALICES_NOTE.to_string()]
    );
}

#[sqlx::test(migrations = false)]
async fn notes_can_be_found_by_their_title(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    assert_eq!(search.matching("bike").await.ids(), [BOBS_NOTE.to_string()]);
}

#[sqlx::test(migrations = false)]
async fn a_search_ignores_case_and_matches_part_of_a_word(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    assert_eq!(
        search.matching("LIC").await.ids(),
        [ALICES_NOTE.to_string()]
    );
}

#[sqlx::test(migrations = false)]
async fn a_search_page_counts_only_the_matching_notes(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    let found = search.matching_page("shopping", 1, 1).await;
    assert_eq!(found.ids(), [ALICES_NOTE.to_string()]);
    assert_eq!(found.total, 2);
}

#[sqlx::test(migrations = false)]
async fn wildcards_in_a_search_match_only_themselves(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    assert_eq!(search.matching("%").await.total, 0);
    assert_eq!(search.matching("_").await.total, 0);
}

#[sqlx::test(migrations = false)]
async fn a_blank_search_finds_every_note(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    assert_eq!(search.matching("   ").await.total, 3);
}

#[sqlx::test(migrations = false)]
async fn a_search_matching_nothing_finds_nothing(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;
    let found = search.matching("zebra").await;
    assert_eq!((found.ids(), found.total), (Vec::<String>::new(), 0));
}
