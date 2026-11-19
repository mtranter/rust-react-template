use crate::driver::{Notes, Refusal};
use app_persistence::Version;
use chrono::TimeDelta;
use notes_api::events::v1::NoteDeleted;
use sqlx::PgPool;
use users_api::seed::USER_ID_ALICE;
use uuid::Uuid;

// The driver's clock starts at midnight UTC; the purge runs nightly at 03:00.

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_nightly_purge_deletes_notes_archived_more_than_30_days_before_and_announces_each(
    pool: PgPool,
) {
    let notes = Notes::seeded(pool).await;
    let old = notes
        .create_archiving_at(USER_ID_ALICE, "Old", notes.now() + TimeDelta::hours(1))
        .await
        .unwrap();
    let recent = notes
        .create_archiving_at(
            USER_ID_ALICE,
            "Recent",
            notes.now() + TimeDelta::days(2) + TimeDelta::hours(1),
        )
        .await
        .unwrap();
    notes.later(TimeDelta::hours(1));
    notes.run_pending_events().await;
    let old_archived_at = notes.now();
    let old = old.archived(old_archived_at);
    notes.later(TimeDelta::days(2));
    notes.run_pending_events().await;
    let recent = recent.archived(notes.now());

    // To 03:00, 31 days after the old one was archived and 29 after the recent one.
    notes.later(TimeDelta::days(29) + TimeDelta::hours(2));
    notes.run_pending_events().await;

    assert_eq!(notes.get(old.id()).await, Err(Refusal::NotFound));
    assert_eq!(notes.get(recent.id()).await, Ok(recent));
    let deleted: Vec<_> = notes
        .announced::<NoteDeleted>()
        .await
        .into_iter()
        .map(|(version, payload)| (version, payload.entity.id, payload.entity.archived_at))
        .collect();
    assert_eq!(
        deleted,
        [(
            Version::from_i64(3),
            old.id().parse::<Uuid>().unwrap(),
            Some(old_archived_at)
        )]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_archived_note_waits_for_the_first_nightly_purge_past_its_30_days(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let note = notes
        .create_archiving_at(USER_ID_ALICE, "Old", notes.now() + TimeDelta::hours(4))
        .await
        .unwrap();
    notes.later(TimeDelta::hours(4));
    notes.run_pending_events().await;
    let note = note.archived(notes.now());

    // 03:00, 29 days and 23 hours after it was archived at 04:00.
    notes.later(TimeDelta::days(29) + TimeDelta::hours(23));
    notes.run_pending_events().await;
    assert_eq!(notes.get(note.id()).await.as_ref(), Ok(&note));

    notes.later(TimeDelta::days(1));
    notes.run_pending_events().await;
    assert_eq!(notes.get(note.id()).await, Err(Refusal::NotFound));
}
