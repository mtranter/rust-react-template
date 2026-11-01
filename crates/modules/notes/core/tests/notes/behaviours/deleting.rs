use crate::driver::{Notes, Refusal, announced};
use app_persistence::Version;
use notes::NoteId;
use notes::seed::{self, ALICES_NOTE};
use notes_api::events::v1::NoteDeleted;
use sqlx::PgPool;
use users_api::seed::ALICE;

#[sqlx::test(migrations = false)]
async fn a_deleted_note_is_gone(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    notes.delete(ALICES_NOTE, 1).await.unwrap();

    assert_eq!(notes.get(ALICES_NOTE).await, Err(Refusal::NotFound));
    assert!(!notes.list().await.ids().contains(&ALICES_NOTE.to_string()));
}

#[sqlx::test(migrations = false)]
async fn deleting_a_note_announces_it_as_it_was_last(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    notes.delete(ALICES_NOTE, 1).await.unwrap();

    assert_eq!(
        notes.announced::<NoteDeleted>().await,
        [(Version::from_i64(2), announced(&seed::alices_note()))]
    );
}

#[sqlx::test(migrations = false)]
async fn a_delete_against_a_stale_version_is_refused_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(notes.delete(ALICES_NOTE, 99).await, Err(Refusal::Stale));
    assert_eq!(notes.get(ALICES_NOTE).await, Ok(seed::alices_note().into()));
    assert!(notes.announced::<NoteDeleted>().await.is_empty());
}

#[sqlx::test(migrations = false)]
async fn a_delete_without_a_precondition_is_refused_and_changes_nothing(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(
        notes.delete_without_precondition(ALICES_NOTE).await,
        Err(Refusal::PreconditionRequired)
    );
    assert_eq!(notes.get(ALICES_NOTE).await, Ok(seed::alices_note().into()));
    assert!(notes.announced::<NoteDeleted>().await.is_empty());
}

#[sqlx::test(migrations = false)]
async fn deleting_an_unknown_note_is_refused_as_not_found(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    assert_eq!(notes.delete(NoteId::new(), 1).await, Err(Refusal::NotFound));
    assert!(notes.announced::<NoteDeleted>().await.is_empty());
}

#[sqlx::test(migrations = false)]
async fn an_archived_note_cannot_be_deleted(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    notes.user_deleted(ALICE).await;
    notes.run_pending_events().await;
    let archived = notes.get(ALICES_NOTE).await.unwrap();

    assert_eq!(notes.delete(ALICES_NOTE, 2).await, Err(Refusal::Conflict));
    assert_eq!(notes.get(ALICES_NOTE).await, Ok(archived));
    assert!(notes.announced::<NoteDeleted>().await.is_empty());
}
