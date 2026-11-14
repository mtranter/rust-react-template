use crate::driver::App;
use chrono::TimeDelta;
use notes_api::events::v1::{NoteArchived, NoteCreated};
use sqlx::PgPool;
use users_api::events::v1::{UserDeleted, UserDisplayNameChanged};
use users_api::seed::USER_ID_ALICE;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn migrate_registers_every_modules_topic_so_the_worker_compacts_its_history(pool: PgPool) {
    let app = App::seeded(pool).await;
    let alice = app.as_user(USER_ID_ALICE);
    let note = alice.write_note("Groceries", "Milk").await.unwrap();
    alice.rename_myself("Alice Liddell").await.unwrap();
    alice.delete_account().await.unwrap();
    app.run_pending_events().await;
    let (alice, note) = (USER_ID_ALICE.to_string(), note.id().to_owned());
    assert!(
        app.keys_announcing::<UserDisplayNameChanged>()
            .await
            .contains(&alice)
    );
    assert!(app.keys_announcing::<NoteCreated>().await.contains(&note));

    app.later(TimeDelta::days(8));
    app.compact().await;

    assert!(
        !app.keys_announcing::<UserDisplayNameChanged>()
            .await
            .contains(&alice)
    );
    assert!(app.keys_announcing::<UserDeleted>().await.contains(&alice));
    assert!(!app.keys_announcing::<NoteCreated>().await.contains(&note));
    assert!(app.keys_announcing::<NoteArchived>().await.contains(&note));
}
