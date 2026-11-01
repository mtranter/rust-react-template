//! A reaction across modules on the composed server: a real deletion in users,
//! delivered by the real worker over every registered module, archives the
//! user's notes. Each module's own tests stand in for the other side.

use app_domain::SystemClock;
use app_module::Context;
use app_web::JwtVerifier;
use notes_api::seed::{ALICES_NOTE, BOBS_NOTE};
use poem::Endpoint;
use poem::test::TestClient;
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use users_api::seed::ALICE;

async fn note(client: &TestClient<impl Endpoint>, id: impl std::fmt::Display) -> Value {
    let response = client.get(format!("/api/notes/{id}")).send().await;
    response.assert_status_is_ok();
    response.0.into_body().into_json().await.expect("a note")
}

#[sqlx::test(migrations = false)]
async fn deleting_a_user_archives_their_notes_once_the_worker_runs(pool: PgPool) {
    app_db::run_migrations(&pool, &server::module_migrations())
        .await
        .expect("migrate every module");
    for module in server::modules() {
        module.seed(pool.clone()).await.expect("seed every module");
    }
    let ctx = Context::new(pool, Arc::new(SystemClock));
    let worker = server::worker(&ctx);
    let verifier = JwtVerifier::new("test-secret");
    let alice = format!("Bearer {}", verifier.issue(ALICE.to_string(), 60).unwrap());
    let client = TestClient::new(server::build_app(&ctx, verifier, "http://localhost"));

    client
        .delete("/api/users/me")
        .header("Authorization", &alice)
        .header("If-Match", "\"1\"")
        .send()
        .await
        .assert_status(poem::http::StatusCode::NO_CONTENT);
    assert!(note(&client, ALICES_NOTE).await.get("archivedAt").is_none());

    worker.run_until_idle().await.expect("the worker runs");

    assert!(note(&client, ALICES_NOTE).await["archivedAt"].is_string());
    assert!(note(&client, BOBS_NOTE).await.get("archivedAt").is_none());
    assert!(worker.dead_letters().await.unwrap().is_empty());
}
