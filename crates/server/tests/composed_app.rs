//! Boots the app as `main` does, over every registered module. Each module's own
//! tests wire it up themselves, so only this catches a module whose provider or
//! migrations never reach the composed server: its operations answer 500.

use app_domain::SystemClock;
use app_module::Context;
use app_web::JwtVerifier;
use poem::test::TestClient;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::BTreeSet;
use std::sync::Arc;

/// Stands in for every path parameter: well-formed, so a lookup reaches the
/// service and database, and matching nothing, so it ends in a not-found.
const NIL_ID: &str = "00000000-0000-0000-0000-000000000000";

#[sqlx::test(migrations = false)]
async fn every_module_answers_its_reads_without_a_server_error(pool: PgPool) {
    app_db::run_migrations(&pool, &server::module_migrations())
        .await
        .expect("migrate every module");
    for module in server::modules() {
        module.seed(pool.clone()).await.expect("seed every module");
    }

    let verifier = JwtVerifier::new("test-secret");
    let bearer = format!(
        "Bearer {}",
        verifier.issue(NIL_ID, 60).expect("issue token")
    );
    let ctx = Context::new(pool, Arc::new(SystemClock));
    let client = TestClient::new(server::build_app(&ctx, verifier, "http://localhost"));

    let spec: Value = serde_json::from_str(&server::build_service().spec()).expect("spec is JSON");
    let mut unread: BTreeSet<&str> = spec["tags"]
        .as_array()
        .expect("spec lists its tags")
        .iter()
        .map(|tag| tag["name"].as_str().expect("a tag has a name"))
        .collect();

    for (path, operations) in spec["paths"].as_object().expect("spec lists its paths") {
        let Some(read) = operations.get("get") else {
            continue;
        };
        let uri = format!("{}{}", server::API_BASE, with_params(path));
        let response = client
            .get(&uri)
            .header("Authorization", &bearer)
            .send()
            .await;
        let status = response.0.status();
        assert!(!status.is_server_error(), "{uri} answered {status}");

        for tag in read["tags"].as_array().into_iter().flatten() {
            unread.remove(tag.as_str().expect("a tag is a name"));
        }
    }

    // A tag is a module's operations; one with no read would leave its
    // wiring untested here.
    assert!(
        unread.is_empty(),
        "no read operation to probe for {unread:?}"
    );
}

fn with_params(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if segment.starts_with('{') {
                NIL_ID
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}
