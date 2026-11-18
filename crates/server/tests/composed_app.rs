//! Boots the app as `main` does, over every registered module. Each module's own
//! tests wire it up themselves, so only this catches a module whose provider or
//! migrations never reach the composed server: its operations answer 500.

use app_authz::{Grant, Policies, PolicyFile, Resource, Scope};
use app_domain::SystemClock;
use app_module::{Context, Module};
use app_test_db::Template;
use app_web::mcp::{self, Discovery, Surface};
use app_web::testing::{Refused, TestApi};
use app_web::{JwtVerifier, Provide, PublicUrl};
use poem::test::TestClient;
use serde_json::Value;
use server::{Composition, PUBLIC, Registered};
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Stands in for every path parameter: well-formed, so a lookup reaches the
/// service and database, and matching nothing, so it ends in a not-found.
const TEMPLATE: Template = Template::new("composed_app", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &server::module_migrations::<Registered>())
            .await
            .expect("migrate every module");
        for module in Registered::modules() {
            module.seed(pool.clone()).await.expect("seed every module");
        }
    })
});

const NIL_ID: &str = "00000000-0000-0000-0000-000000000000";

/// As `main` builds it: every module's policies compiled into its authorizer.
fn context(pool: PgPool) -> Context {
    let authorizer = server::authorizer::<Registered>().expect("every module's policies load");
    Context::new(pool, Arc::new(SystemClock)).with_authorizer(Arc::new(authorizer))
}

#[app_test_db::test(TEMPLATE)]
async fn every_module_answers_its_reads_without_a_server_error(pool: PgPool) {
    let verifier = JwtVerifier::new("test-secret");
    let bearer = format!(
        "Bearer {}",
        verifier.issue(NIL_ID, &["user"], 60).expect("issue token")
    );
    let ctx = context(pool);
    // A read module serving its own copy answers once the worker has caught it up.
    server::register_subscriptions::<Registered>(&ctx)
        .await
        .expect("register every module's subscriptions");
    server::worker::<Registered>(&ctx)
        .run_until_idle()
        .await
        .expect("the worker reaches the database");
    let client = TestClient::new(server::build_app::<Registered>(
        &ctx,
        verifier,
        Discovery::default(),
        "http://localhost",
    ));

    let spec: Value =
        serde_json::from_str(&server::build_service::<Registered>().spec()).expect("spec is JSON");
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
    let token = verifier.issue(NIL_ID, &["user"], 60).expect("issue token");
    let ctx = context(pool);
    let api = TestApi::over(server::build_app::<Registered>(
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
        app_web::mcp::problems::<server::Tools<Registered>>(),
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
        .filter(|status| {
            !server::Tools::<Registered>::INSTRUCTIONS.contains(&format!("`{status}`"))
        })
        .collect();
    assert!(
        untranslated.is_empty(),
        "the MCP instructions don't say what {untranslated:?} mean as a refusal's code"
    );
}

#[app_test_db::test(TEMPLATE)]
async fn with_an_issuer_mcp_clients_are_told_where_to_sign_in(pool: PgPool) {
    let ctx = context(pool);
    let discovery = Discovery::new(
        PublicUrl::new(Some("https://app.example".into())),
        Some("https://issuer.example".into()),
    );
    let client = TestClient::new(server::build_app::<Registered>(
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

#[test]
fn every_operation_is_guarded_unless_it_is_listed_as_public() {
    for (path, method, operation) in operations() {
        let id = operation["operationId"].as_str().unwrap_or_default();
        let guarded = operation["security"]
            .as_array()
            .is_some_and(|requirements| !requirements.is_empty());
        let public = PUBLIC.iter().any(|(public, _)| *public == id);
        assert!(
            guarded || public,
            "{method} {path} ({id}) has no security requirement: guard it with its module's \
             `Caller`, or list it in `server::PUBLIC` with why"
        );
        assert!(
            !(guarded && public),
            "{method} {path} ({id}) is listed in `server::PUBLIC` but has a security requirement"
        );
    }
    for (id, reason) in PUBLIC {
        assert!(!reason.is_empty(), "`{id}` is public without a reason");
        assert!(
            operations().any(|(_, _, operation)| operation["operationId"] == *id),
            "`{id}` is listed in `server::PUBLIC` but there is no such operation"
        );
    }
}

#[test]
fn every_guarded_operation_names_an_action_its_modules_policies_have() {
    let policies = server::policies::<Registered>().expect("every module's policies load");
    let performs = server::operations::<Registered>();
    for (path, method, operation) in operations() {
        let id = operation["operationId"].as_str().unwrap_or_default();
        if PUBLIC.iter().any(|(public, _)| *public == id) {
            continue;
        }
        let named: Vec<_> = performs.iter().filter(|p| p.operation == id).collect();
        assert_eq!(
            named.len(),
            1,
            "{method} {path} ({id}) is guarded, so its module's `OPERATIONS` names the action \
             it performs, once"
        );
        let performed = named[0];
        let grant = Grant {
            role: "any".into(),
            resource: performed.resource.into(),
            action: performed.action.into(),
            scope: Scope::Any,
        };
        assert!(
            policies.iter().any(|p| p.check(&grant).is_ok()),
            "`{id}` performs `{}:{}`, which no module's policies have",
            performed.resource,
            performed.action
        );
    }
    for performed in &performs {
        assert!(
            operations().any(|(_, _, operation)| operation["operationId"] == performed.operation),
            "`OPERATIONS` names `{}`, but there is no such operation",
            performed.operation
        );
    }
}

/// A module whose default grants name an action its resource doesn't have.
struct MisGranted;

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
enum GadgetAction {
    Make,
}

struct Gadget;

impl Resource for Gadget {
    const KIND: &'static str = "gadget";
    const ATTRS: &'static [&'static str] = &[];
    type Action = GadgetAction;

    fn attr(&self, _: &str) -> Option<app_authz::Value> {
        None
    }
}

struct NoData;

impl Provide for NoData {
    type Data = ();

    fn build(self) {}
}

impl Composition for MisGranted {
    type Endpoints = ();

    fn modules() -> Vec<Module> {
        vec![Module::new(|_| NoData).with_policies(|| {
            Policies::builder("gadgets")
                .resource::<Gadget>(PolicyFile::new(
                    "policies/gadget.yaml",
                    "apiVersion: auth.flagrante.io/v1\nresourcePolicy:\n  resource: gadget\n  version: default\n  rules: []\n",
                ))
                .default_grants(PolicyFile::new(
                    "policies/default_grants.yaml",
                    "apiVersion: auth.flagrante.io/v1\ndefaultGrants:\n  - { role: user, resource: gadget, action: frobnicate }\n",
                ))
        })]
    }
}

#[test]
fn a_default_grant_of_an_action_the_resource_lacks_stops_the_server_naming_it() {
    let refused = server::authorizer::<MisGranted>()
        .expect_err("the server refuses to start")
        .to_string();
    for named in [
        "`gadgets`",
        "policies/default_grants.yaml:3",
        "`frobnicate`",
    ] {
        assert!(refused.contains(named), "{refused:?} doesn't name {named}");
    }
    assert!(
        server::policies::<MisGranted>().is_err(),
        "nor does `migrate` start"
    );
}

#[test]
fn every_modules_policies_load() {
    let policies: Vec<Policies> = server::policies::<Registered>().expect("load");
    assert_eq!(policies.len(), Registered::modules().len());
}

/// Every operation of the spec the server serves.
fn operations() -> impl Iterator<Item = (String, String, Value)> {
    let spec: Value =
        serde_json::from_str(&server::build_service::<Registered>().spec()).expect("spec is JSON");
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
