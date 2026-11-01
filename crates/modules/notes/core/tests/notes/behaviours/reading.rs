use crate::driver::{Notes, Refusal};
use notes::NoteId;
use notes::seed::{self, ALICES_NOTE};
use sqlx::PgPool;

#[sqlx::test(migrations = false)]
async fn a_note_can_be_found_by_its_id(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(notes.get(ALICES_NOTE).await, Ok(seed::alices_note().into()));
}

#[sqlx::test(migrations = false)]
async fn an_unknown_note_is_not_found(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(notes.get(NoteId::new()).await, Err(Refusal::NotFound));
}

#[sqlx::test(migrations = false)]
async fn a_malformed_note_id_is_invalid(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(notes.get("not-a-note-id").await, Err(Refusal::invalid([])));
}
