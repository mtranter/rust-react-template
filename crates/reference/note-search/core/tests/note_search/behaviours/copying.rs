use crate::driver::{NoteSearch, NoteSummaryJson, Refusal};
use notes_api::seed::{self, ALICE_NOTE_ID_SHOPPING_LIST, NOTE_ID_ORPHANED};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn nothing_is_served_until_the_copy_has_caught_up_with_what_was_published_before(
    pool: PgPool,
) {
    let search = NoteSearch::added_after_the_seeds(pool).await;
    assert_eq!(search.search(&[]).await.err(), Some(Refusal::Unavailable));

    search.run_pending_events().await;

    assert_eq!(search.all().await.total, 3);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_renamed_author_is_found_by_their_new_name(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;

    search
        .user_renamed(users_api::seed::alice(), "Alicia")
        .await;

    assert_eq!(
        search.matching("alicia").await.ids(),
        [ALICE_NOTE_ID_SHOPPING_LIST.to_string()]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_retitled_note_is_found_by_its_new_title(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;

    search
        .note_retitled(seed::orphaned_note(), "Zebra facts")
        .await;

    assert_eq!(
        search.matching("zebra").await.ids(),
        [NOTE_ID_ORPHANED.to_string()]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_deleted_note_is_no_longer_found(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;

    search.note_deleted(seed::alices_note()).await;

    assert_eq!(search.matching("shopping").await.total, 1);
    assert_eq!(search.all().await.total, 2);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_deleted_authors_notes_are_listed_without_an_author_name(pool: PgPool) {
    let search = NoteSearch::seeded(pool).await;

    search.user_deleted(users_api::seed::alice()).await;

    assert_eq!(
        search.all().await.item(ALICE_NOTE_ID_SHOPPING_LIST),
        &NoteSummaryJson::of(seed::alices_note(), None)
    );
}
