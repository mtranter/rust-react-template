//! Boots the app as `main` does, over every registered module. Each module's own
//! tests wire it up themselves, so only this catches a module whose provider or
//! migrations never reach the composed server: its operations answer 500.

use app_domain::SystemClock;
use app_module::Context;
use app_test_db::Template;
use app_web::mcp::{self, Discovery, Surface};
use app_web::testing::{Refused, TestApi};
use app_web::{JwtVerifier, PublicUrl};
use poem::test::TestClient;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Stands in for every path parameter: well-formed, so a lookup reaches the
/// service and database, and matching nothing, so it ends in a not-found.
const TEMPLATE: Template = Template::new("composed_app", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &server::module_migrations())
            .await
            .expect("migrate every module");
        for module in server::modules() {
            module.seed(pool.clone()).await.expect("seed every module");
        }
    })
});

const NIL_ID: &str = "00000000-0000-0000-0000-000000000000";

#[app_test_db::test(TEMPLATE)]
async fn every_module_answers_its_reads_without_a_server_error(pool: PgPool) {
    let verifier = JwtVerifier::new("test-secret");
    let bearer = format!(
        "Bearer {}",
        verifier.issue(NIL_ID, 60).expect("issue token")
    );
    let ctx = Context::new(pool, Arc::new(SystemClock));
    // A read module serving its own copy answers once the worker has caught it up.
    server::register_subscriptions(&ctx)
        .await
        .expect("register every module's subscriptions");
    server::worker(&ctx)
        .run_until_idle()
        .await
        .expect("the worker reaches the database");
    let client = TestClient::new(server::build_app(
        &ctx,
        verifier,
        Discovery::default(),
        "http://localhost",
    ));

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

/// The MCP surface, so a change to it shows in a diff: every operation's id
/// but those `REST_ONLY`. `make new-module` adds a new module's here.
const TOOLS: &[&str] = &[
    "register_user",
    "list_users",
    "get_me",
    "rename_me",
    "delete_me",
    "get_user",
    "create_note",
    "list_notes",
    "get_note",
    "edit_note",
    "delete_note",
    "add_note_item",
    "remove_note_item",
    "search_notes",
];

#[derive(Debug)]
struct Code(#[allow(dead_code)] String);

impl From<Refused> for Code {
    fn from(refused: Refused) -> Self {
        Self(refused.code)
    }
}

#[app_test_db::test(TEMPLATE)]
async fn one_mcp_endpoint_offers_a_tool_per_operation(pool: PgPool) {
    let verifier = JwtVerifier::new("test-secret");
    let token = verifier.issue(NIL_ID, 60).expect("issue token");
    let ctx = Context::new(pool, Arc::new(SystemClock));
    let api = TestApi::over(server::build_app(
        &ctx,
        verifier,
        Discovery::default(),
        "http://localhost",
    ));
    let session = api.mcp_session::<Code>(Some(&token)).await.unwrap();

    let mut offered = session.tool_names::<Code>().await.unwrap();
    offered.sort();
    let mut expected: Vec<_> = TOOLS.iter().map(ToString::to_string).collect();
    expected.sort();
    assert_eq!(offered, expected);
    assert_eq!(
        app_web::mcp::problems::<server::Tools>(),
        Vec::<String>::new()
    );
}

#[test]
fn every_operation_has_a_unique_id_and_a_summary() {
    let mut ids = BTreeMap::new();
    for (path, method, operation) in operations() {
        let at = format!("{method} {path}");
        let id = operation["operationId"]
            .as_str()
            .unwrap_or_else(|| panic!("{at} has no operation id"));
        assert!(
            operation["summary"].as_str().is_some_and(|s| !s.is_empty()),
            "{at} has no summary"
        );
        if let Some(other) = ids.insert(id.to_owned(), at.clone()) {
            panic!("{at} and {other} share the operation id {id}");
        }
    }
}

#[test]
fn the_instructions_say_what_every_refusal_status_means() {
    let statuses: BTreeSet<String> = operations()
        .flat_map(|(_, _, operation)| {
            operation["responses"]
                .as_object()
                .expect("an operation declares its responses")
                .keys()
                .cloned()
                .collect::<Vec<_>>()
        })
        .filter(|status| !status.starts_with('2'))
        .collect();
    let untranslated: Vec<_> = statuses
        .iter()
        .filter(|status| !server::Tools::INSTRUCTIONS.contains(&format!("`{status}`")))
        .collect();
    assert!(
        untranslated.is_empty(),
        "the MCP instructions don't say what {untranslated:?} mean as a refusal's code"
    );
}

#[app_test_db::test(TEMPLATE)]
async fn with_an_issuer_mcp_clients_are_told_where_to_sign_in(pool: PgPool) {
    let ctx = Context::new(pool, Arc::new(SystemClock));
    let discovery = Discovery::new(
        PublicUrl::new(Some("https://app.example".into())),
        Some("https://issuer.example".into()),
    );
    let client = TestClient::new(server::build_app(
        &ctx,
        JwtVerifier::new("test-secret"),
        discovery,
        "http://localhost",
    ));

    for path in [
        mcp::METADATA_PATH.to_owned(),
        format!("{}{}", mcp::METADATA_PATH, mcp::PATH),
    ] {
        let response = client.get(&path).send().await;
        response.assert_status_is_ok();
        let body = response.json().await;
        body.value()
            .object()
            .get("resource")
            .assert_string("https://app.example/mcp");
    }
    client
        .post(mcp::PATH)
        .send()
        .await
        .assert_header_exist("www-authenticate");
}

/// Every operation of the spec the server serves.
fn operations() -> impl Iterator<Item = (String, String, Value)> {
    let spec: Value = serde_json::from_str(&server::build_service().spec()).expect("spec is JSON");
    spec["paths"]
        .as_object()
        .expect("spec lists its paths")
        .iter()
        .flat_map(|(path, operations)| {
            operations
                .as_object()
                .expect("a path lists its operations")
                .iter()
                .map(|(method, operation)| (path.clone(), method.to_uppercase(), operation.clone()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
        .into_iter()
}
