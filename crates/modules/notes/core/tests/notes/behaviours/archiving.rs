use crate::driver::{NoteJson, Notes, Refusal, announced};
use app_events::EventPayload;
use app_persistence::Version;
use chrono::TimeDelta;
use notes::NoteId;
use notes::seed::{self, ALICE_NOTE_ID_SHOPPING_LIST, BOB_NOTE_ID_BIKE_REPAIRS};
use notes_api::events::v1::{self, NoteArchived};
use sqlx::PgPool;
use users_api::seed::USER_ID_ALICE;

fn alices_note() -> NoteJson {
    seed::alices_note().into()
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_deleted_authors_notes_are_archived(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    notes.user_deleted(USER_ID_ALICE).await;
    notes.run_pending_events().await;

    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(alices_note().archived(notes.now()))
    );
    assert_eq!(
        notes.get(BOB_NOTE_ID_BIKE_REPAIRS).await,
        Ok(seed::bobs_note().into())
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn archiving_a_note_announces_it(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    notes.user_deleted(USER_ID_ALICE).await;
    notes.run_pending_events().await;

    assert_eq!(
        notes.announced::<NoteArchived>().await,
        [(
            Version::from_i64(2),
            EventPayload::new(v1::Note {
                archived_at: Some(notes.now()),
                ..announced(&seed::alices_note()).entity
            })
        )]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn archiving_announces_each_note_at_its_new_version(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let newer = notes.create(USER_ID_ALICE, "Newer", "").await.unwrap();
    let newer_id: NoteId = newer.id().parse().unwrap();
    notes.edit(newer_id, 1, "Newer", "edited").await.unwrap();

    notes.user_deleted(USER_ID_ALICE).await;
    notes.run_pending_events().await;

    let archived: Vec<_> = notes
        .announced::<NoteArchived>()
        .await
        .into_iter()
        .map(|(version, payload)| (version, payload.entity.id))
        .collect();
    assert_eq!(
        archived,
        [
            (Version::from_i64(2), ALICE_NOTE_ID_SHOPPING_LIST.as_uuid()),
            (Version::from_i64(3), newer_id.as_uuid()),
        ]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn notes_are_archived_only_once_the_deletion_is_delivered(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    notes.user_deleted(USER_ID_ALICE).await;

    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(alices_note())
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_deletion_delivered_twice_archives_once(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let archived_at = notes.now();

    notes.user_deleted(USER_ID_ALICE).await;
    notes.run_pending_events().await;
    notes.later(TimeDelta::hours(1));
    notes.everything_is_delivered_again().await;
    notes.run_pending_events().await;

    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(alices_note().archived(archived_at))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_archived_note_cannot_be_edited(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    notes.user_deleted(USER_ID_ALICE).await;
    notes.run_pending_events().await;
    let archived = notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await.unwrap();

    assert_eq!(
        notes
            .edit(ALICE_NOTE_ID_SHOPPING_LIST, 2, "Renamed", "")
            .await,
        Err(Refusal::Conflict)
    );
    assert_eq!(notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await, Ok(archived));
}

/// Archiving is one transaction over all the author's notes: Alice's seeded note
/// is saved first and her newer one second, so the failure comes after a save.
#[app_test_db::test(crate::driver::TEMPLATE)]
async fn if_one_note_fails_to_archive_none_are_until_the_retry_succeeds(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let newer = notes.create(USER_ID_ALICE, "Newer", "").await.unwrap();
    notes.saving_fails_for(newer.id()).await;

    notes.user_deleted(USER_ID_ALICE).await;
    notes.run_pending_events().await;

    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(alices_note())
    );
    assert_eq!(notes.get(newer.id()).await.as_ref(), Ok(&newer));
    assert!(notes.announced::<NoteArchived>().await.is_empty());

    notes.saving_works_again().await;
    notes.later(TimeDelta::minutes(1));
    let archived_at = notes.now();
    notes.run_pending_events().await;

    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(alices_note().archived(archived_at))
    );
    assert_eq!(notes.get(newer.id()).await, Ok(newer.archived(archived_at)));
}
