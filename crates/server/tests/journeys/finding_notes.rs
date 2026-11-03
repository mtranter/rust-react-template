use crate::driver::{App, NoteSummaryJson};
use sqlx::PgPool;
use users_api::seed::ALICE;

#[sqlx::test(migrations = false)]
async fn a_note_is_found_by_its_authors_current_name(pool: PgPool) {
    let app = App::seeded(pool).await;
    let alice = app.as_user(ALICE);

    let written = alice.write_note("Garden plan", "tomatoes").await.unwrap();
    let found = app.search_notes("alice").await;
    assert_eq!(
        found.item(written.id()),
        &NoteSummaryJson::of(&written, &alice.me().await.unwrap())
    );

    let renamed = alice.rename_myself("Ally").await.unwrap();
    assert_eq!(alice.me().await.as_ref(), Ok(&renamed));

    let found = app.search_notes("ally").await;
    assert_eq!(
        found.item(written.id()),
        &NoteSummaryJson::of(&written, &renamed)
    );
    assert_eq!(app.search_notes("alice").await.ids(), Vec::<String>::new());
}
