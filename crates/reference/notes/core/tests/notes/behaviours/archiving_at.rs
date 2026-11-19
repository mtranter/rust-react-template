use crate::driver::{Notes, Refusal};
use app_persistence::Version;
use app_test_support::{assert_json_matches, assert_json_not_exists};
use chrono::TimeDelta;
use notes::seed::ALICE_NOTE_ID_SHOPPING_LIST;
use notes_api::events::v1::NoteArchived;
use sqlx::PgPool;
use users_api::seed::USER_ID_ALICE;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_note_keeps_the_archive_at_it_was_created_with(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let at = notes.now() + TimeDelta::days(3);

    let created = notes
        .create_archiving_at(USER_ID_ALICE, "Trip", at)
        .await
        .unwrap();

    assert_json_matches!(created, "$.archiveAt", at.to_rfc3339());
    assert_json_not_exists!(created, "$.archivedAt");
    assert_eq!(notes.get(created.id()).await, Ok(created));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_note_archives_itself_at_its_archive_at_and_announces_it(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let at = notes.now() + TimeDelta::hours(1);
    let created = notes
        .create_archiving_at(USER_ID_ALICE, "Trip", at)
        .await
        .unwrap();

    notes.later(TimeDelta::minutes(59));
    notes.run_pending_events().await;
    assert_eq!(notes.get(created.id()).await.as_ref(), Ok(&created));

    notes.later(TimeDelta::minutes(1));
    notes.run_pending_events().await;

    assert_eq!(notes.get(created.id()).await, Ok(created.archived(at)));
    let archived: Vec<_> = notes
        .announced::<NoteArchived>()
        .await
        .into_iter()
        .map(|(version, payload)| (version, payload.entity.archived_at))
        .collect();
    assert_eq!(archived, [(Version::from_i64(2), Some(at))]);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn changing_the_archive_at_reschedules_the_archive(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let first = notes.now() + TimeDelta::hours(1);
    let second = notes.now() + TimeDelta::hours(2);
    let created = notes
        .create_archiving_at(USER_ID_ALICE, "Trip", first)
        .await
        .unwrap();

    let edited = notes
        .edit_archiving_at(created.id(), 1, "Trip", Some(second))
        .await
        .unwrap();
    assert_json_matches!(edited, "$.archiveAt", second.to_rfc3339());

    notes.later(TimeDelta::hours(1));
    notes.run_pending_events().await;
    assert_eq!(notes.get(created.id()).await.as_ref(), Ok(&edited));

    notes.later(TimeDelta::hours(1));
    notes.run_pending_events().await;
    assert_eq!(notes.get(created.id()).await, Ok(edited.archived(second)));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_archive_at_set_by_an_edit_archives_the_note_then(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let at = notes.now() + TimeDelta::hours(1);

    let edited = notes
        .edit_archiving_at(ALICE_NOTE_ID_SHOPPING_LIST, 1, "Shopping", Some(at))
        .await
        .unwrap();
    notes.later(TimeDelta::hours(1));
    notes.run_pending_events().await;

    assert_eq!(
        notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(edited.archived(at))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn clearing_the_archive_at_cancels_the_archive(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let at = notes.now() + TimeDelta::hours(1);
    let created = notes
        .create_archiving_at(USER_ID_ALICE, "Trip", at)
        .await
        .unwrap();

    let edited = notes
        .edit_archiving_at(created.id(), 1, "Trip", None)
        .await
        .unwrap();
    notes.later(TimeDelta::hours(2));
    notes.run_pending_events().await;

    assert_json_not_exists!(edited, "$.archiveAt");
    assert_eq!(notes.get(created.id()).await, Ok(edited));
    assert!(notes.announced::<NoteArchived>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_stale_edit_of_the_archive_at_leaves_the_archive_as_it_was(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let at = notes.now() + TimeDelta::hours(1);
    let created = notes
        .create_archiving_at(USER_ID_ALICE, "Trip", at)
        .await
        .unwrap();
    notes
        .edit_archiving_at(created.id(), 1, "Renamed", Some(at))
        .await
        .unwrap();

    assert_eq!(
        notes.edit_archiving_at(created.id(), 1, "Trip", None).await,
        Err(Refusal::Stale)
    );
    notes.later(TimeDelta::hours(1));
    notes.run_pending_events().await;

    assert_json_matches!(
        notes.get(created.id()).await.unwrap(),
        "$.archivedAt",
        at.to_rfc3339()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_note_already_archived_by_its_time_stays_as_it_was(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let created = notes
        .create_archiving_at(USER_ID_ALICE, "Trip", notes.now() + TimeDelta::hours(1))
        .await
        .unwrap();
    notes.user_deleted(USER_ID_ALICE).await;
    notes.run_pending_events().await;
    let archived = notes.get(created.id()).await.unwrap();

    notes.later(TimeDelta::hours(1));
    notes.run_pending_events().await;

    assert_eq!(notes.get(created.id()).await, Ok(archived));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_note_deleted_before_its_time_is_not_archived(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let created = notes
        .create_archiving_at(USER_ID_ALICE, "Trip", notes.now() + TimeDelta::hours(1))
        .await
        .unwrap();

    notes.delete(created.id(), 1).await.unwrap();
    notes.later(TimeDelta::hours(1));
    notes.run_pending_events().await;

    assert_eq!(notes.get(created.id()).await, Err(Refusal::NotFound));
    assert!(notes.announced::<NoteArchived>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_archive_at_that_is_not_a_time_is_refused_naming_it(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let before = notes.list().await.total;

    assert_eq!(
        notes
            .create_archiving_at_raw(USER_ID_ALICE, "Trip", "tomorrow")
            .await,
        Err(Refusal::invalid(["archiveAt"]))
    );
    assert_eq!(notes.list().await.total, before);
}
