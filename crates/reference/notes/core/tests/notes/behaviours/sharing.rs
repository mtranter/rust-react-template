use crate::driver::{NOTICEBOARD_TOKEN, Notes, Refusal};
use fake_noticeboard::Pin;
use notes::seed::{ALICE_NOTE_ID_SHOPPING_LIST, BOB_NOTE_ID_BIKE_REPAIRS};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_shared_note_is_pinned_to_the_noticeboard_by_the_worker(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let board = notes.noticeboard_connected().await;
    let note = notes.get(ALICE_NOTE_ID_SHOPPING_LIST).await.unwrap();

    assert_eq!(notes.share(ALICE_NOTE_ID_SHOPPING_LIST).await, Ok(note));
    assert!(board.pins().is_empty(), "pinned in the request");

    notes.run_pending_events().await;

    let pinned = board.pins();
    assert_eq!(
        pinned.get(&ALICE_NOTE_ID_SHOPPING_LIST.to_string()),
        Some(&Pin {
            title: "Shopping list".into(),
            body: "for the weekend".into(),
        })
    );
    assert_eq!(notes.failed_jobs().await, Vec::<String>::new());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn sharing_twice_before_the_worker_runs_pins_once(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let board = notes.noticeboard_connected().await;

    notes.share(ALICE_NOTE_ID_SHOPPING_LIST).await.unwrap();
    notes.share(ALICE_NOTE_ID_SHOPPING_LIST).await.unwrap();
    notes.run_pending_events().await;

    assert_eq!(board.puts(), 1);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn another_users_note_cant_be_shared(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let board = notes.noticeboard_connected().await;

    assert_eq!(
        notes.share(BOB_NOTE_ID_BIKE_REPAIRS).await,
        Err(Refusal::NotFound)
    );
    notes.run_pending_events().await;
    assert!(board.pins().is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_share_waits_to_retry_while_the_noticeboard_is_not_connected(pool: PgPool) {
    let notes = Notes::seeded(pool).await;

    notes.share(ALICE_NOTE_ID_SHOPPING_LIST).await.unwrap();
    notes.run_pending_events().await;

    assert_eq!(notes.failed_jobs().await, ["notes.share_note"]);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_share_the_noticeboard_refuses_waits_to_retry(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let board = fake_noticeboard::Noticeboard::new(NOTICEBOARD_TOKEN);
    let url = board.serve();
    notes.noticeboard_at(&url, "a-token-it-never-issued").await;

    notes.share(ALICE_NOTE_ID_SHOPPING_LIST).await.unwrap();
    notes.run_pending_events().await;

    assert!(board.pins().is_empty());
    assert_eq!(notes.failed_jobs().await, ["notes.share_note"]);
}
