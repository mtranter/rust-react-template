use crate::driver::{App, Refusal};
use notes_api::seed::{ALICE_NOTE_ID_SHOPPING_LIST, BOB_NOTE_ID_BIKE_REPAIRS};
use sqlx::PgPool;
use users_api::seed::USER_ID_ALICE;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn deleting_an_account_archives_only_that_users_notes(pool: PgPool) {
    let app = App::seeded(pool).await;
    let alices = app.note(ALICE_NOTE_ID_SHOPPING_LIST).await.unwrap();
    let bobs = app.note(BOB_NOTE_ID_BIKE_REPAIRS).await.unwrap();

    app.as_user(USER_ID_ALICE).delete_account().await.unwrap();
    assert_eq!(
        app.as_user(USER_ID_ALICE).me().await,
        Err(Refusal::NotFound)
    );
    assert_eq!(
        app.note(ALICE_NOTE_ID_SHOPPING_LIST).await.as_ref(),
        Ok(&alices)
    );

    app.run_pending_events().await;

    assert_eq!(
        app.note(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(alices.archived(app.now()))
    );
    assert_eq!(app.note(BOB_NOTE_ID_BIKE_REPAIRS).await, Ok(bobs));
    assert!(app.given_up_on().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_reactions_to_a_deleted_account_are_traced_from_its_request(pool: PgPool) {
    let app = App::seeded(pool).await;
    // The seeds' events were published outside any trace: each is handled in one of its own.
    app.run_pending_events().await;
    let traces = app_web::testing::Traces::install();

    app.as_user(USER_ID_ALICE).delete_account().await.unwrap();
    app.run_pending_events().await;

    let request = traces.span("DELETE /api/users/me");
    let jobs: Vec<_> = traces
        .spans()
        .into_iter()
        .filter(|span| span.name == "job")
        .collect();
    let archiving = jobs
        .iter()
        .find(|job| {
            app_web::testing::Traces::attribute(job, "messaging.destination.name").as_deref()
                == Some("notes.archive_deleted_authors_notes")
        })
        .expect("the deletion was handled");
    assert_eq!(archiving.parent_span_id, request.span_context.span_id());
    // The archiving's own event, and every projection's copy, follow it.
    assert!(jobs.len() > 1);
    for job in &jobs {
        assert_eq!(job.span_context.trace_id(), request.span_context.trace_id());
    }
}
