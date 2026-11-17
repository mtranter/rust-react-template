use crate::driver::{Notes, Refusal};
use notes::NoteId;
use notes::seed::{self, ALICE_NOTE_ID_SHOPPING_LIST};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_note_can_be_found_by_its_id(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(seed::alices_note().into())
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_unknown_note_is_not_found(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(notes.get(NoteId::new()).await, Err(Refusal::NotFound));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_malformed_note_id_is_invalid(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(notes.get("not-a-note-id").await, Err(Refusal::invalid([])));
}
