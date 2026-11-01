use crate::driver::{NoteJson, Notes, Refusal};
use chrono::TimeDelta;
use notes::seed::{self, ALICES_NOTE, BOBS_NOTE};
use sqlx::PgPool;
use users_api::seed::ALICE;

fn alices_note() -> NoteJson {
    seed::alices_note().into()
}

#[sqlx::test(migrations = false)]
async fn a_deleted_authors_notes_are_archived(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    notes.user_deleted(ALICE).await;
    notes.run_pending_events().await;

    assert_eq!(
        notes.get(ALICES_NOTE).await,
        Ok(alices_note().archived(notes.now()))
    );
    assert_eq!(notes.get(BOBS_NOTE).await, Ok(seed::bobs_note().into()));
}

#[sqlx::test(migrations = false)]
async fn notes_are_archived_only_once_the_deletion_is_delivered(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    notes.user_deleted(ALICE).await;

    assert_eq!(notes.get(ALICES_NOTE).await, Ok(alices_note()));
}

#[sqlx::test(migrations = false)]
async fn a_deletion_delivered_twice_archives_once(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let archived_at = notes.now();

    notes.user_deleted(ALICE).await;
    notes.run_pending_events().await;
    notes.later(TimeDelta::hours(1));
    notes.user_deleted(ALICE).await;
    notes.run_pending_events().await;

    assert_eq!(
        notes.get(ALICES_NOTE).await,
        Ok(alices_note().archived(archived_at))
    );
}

#[sqlx::test(migrations = false)]
async fn an_archived_note_cannot_be_edited(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    notes.user_deleted(ALICE).await;
    notes.run_pending_events().await;
    let archived = notes.get(ALICES_NOTE).await.unwrap();

    assert_eq!(
        notes.edit(ALICES_NOTE, 2, "Renamed", "").await,
        Err(Refusal::Conflict)
    );
    assert_eq!(notes.get(ALICES_NOTE).await, Ok(archived));
}

/// Archiving is one transaction over all the author's notes: Alice's seeded note
/// is saved first and her newer one second, so the failure comes after a save.
#[sqlx::test(migrations = false)]
async fn if_one_note_fails_to_archive_none_are_until_the_retry_succeeds(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let newer = notes.create(ALICE, "Newer", "").await.unwrap();
    notes.saving_fails_for(newer.id()).await;

    notes.user_deleted(ALICE).await;
    notes.run_pending_events().await;

    assert_eq!(notes.get(ALICES_NOTE).await, Ok(alices_note()));
    assert_eq!(notes.get(newer.id()).await.as_ref(), Ok(&newer));

    notes.saving_works_again().await;
    notes.later(TimeDelta::minutes(1));
    let archived_at = notes.now();
    notes.run_pending_events().await;

    assert_eq!(
        notes.get(ALICES_NOTE).await,
        Ok(alices_note().archived(archived_at))
    );
    assert_eq!(notes.get(newer.id()).await, Ok(newer.archived(archived_at)));
}
