use crate::driver::{Notes, Refusal};
use app_persistence::Version;
use app_test_support::assert_json_matches;
use notes_api::events::v1::{NoteCreated, NoteDeleted, NoteEdited};
use sqlx::PgPool;
use users_api::seed::ALICE;

#[sqlx::test(migrations = false)]
async fn a_note_can_be_written_revised_and_removed(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    let written = notes.create(ALICE, "Groceries", "milk").await.unwrap();
    assert_eq!(notes.get(written.id()).await.as_ref(), Ok(&written));

    let revised = notes
        .edit(written.id(), written.version(), "Groceries", "milk, eggs")
        .await
        .unwrap();
    assert_json_matches!(revised, "$.body", "milk, eggs");
    assert_eq!(notes.get(written.id()).await.as_ref(), Ok(&revised));

    assert_eq!(
        notes
            .edit(written.id(), written.version(), "Shopping", "")
            .await,
        Err(Refusal::Stale)
    );
    assert_eq!(notes.list().await.item(written.id()), &revised);

    notes.delete(written.id(), revised.version()).await.unwrap();
    assert_eq!(notes.get(written.id()).await, Err(Refusal::NotFound));

    let versions = [
        notes.announced::<NoteCreated>().await[0].0,
        notes.announced::<NoteEdited>().await[0].0,
        notes.announced::<NoteDeleted>().await[0].0,
    ];
    assert_eq!(versions, [1, 2, 3].map(Version::from_i64));
}
