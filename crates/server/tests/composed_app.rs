//! Boots the app as `main` does, over every registered module. Each module's own
//! tests wire it up themselves, so only this catches a module whose provider or
//! migrations never reach the composed server: its operations answer 500.

use app_authz::{Grant, Policies, PolicyFile, Resource, Scope};
use app_domain::SystemClock;
use app_module::{Context, Module};
use app_test_db::Template;
use app_web::mcp::{self, Discovery, Surface};
use app_web::testing::{Metrics, Refused, TestApi, Traces};
use app_web::{JwtVerifier, Provide, PublicUrl};
use poem::test::TestClient;
use serde_json::Value;
use server::sign_in::{self, SignIn};
use server::{Composition, PUBLIC, Registered, SIGNED_IN};
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
        let ctx = Context::new(pool.clone(), Arc::new(SystemClock));
        server::apply_default_grants::<Registered>(&ctx)
            .await
            .expect("apply every module's default grants");
        for module in Registered::modules() {
            module.seed(&ctx).await.expect("seed every module");
        }
    })
});

const NIL_ID: &str = "00000000-0000-0000-0000-000000000000";

/// As `main` builds it: every module's policies and the stored grants compiled
/// into its authorizer, with `NIL_ID` a bootstrap admin, so `access` reads its
/// tables rather than refusing at its gate.
async fn context(pool: PgPool) -> Context {
    let authorizer = server::authorizer::<Registered>()
        .expect("every module's policies load")
        .with_bootstrap_admins([NIL_ID.to_owned()]);
    let ctx = Context::new(pool, Arc::new(SystemClock)).with_authorizer(Arc::new(authorizer));
    server::rules::<Registered>(&ctx)
        .expect("every module's policies load")
        .refresh()
        .await
        .expect("the grants are read");
    ctx
}

#[app_test_db::test(TEMPLATE)]
async fn every_module_answers_its_reads_without_a_server_error(pool: PgPool) {
    let verifier = JwtVerifier::new("test-secret");
    let bearer = format!(
        "Bearer {}",
        verifier.issue(NIL_ID, 60).expect("issue token")
    );
    let ctx = context(pool).await;
    // A read module serving its own copy answers once the worker has caught it up.
    server::register_subscriptions::<Registered>(&ctx)
        .await
        .expect("register every module's subscriptions");
    server::register_schedules::<Registered>(&ctx)
        .await
        .expect("register every module's schedules");
    server::worker::<Registered>(&ctx)
        .run_until_idle()
        .await
        .expect("the worker reaches the database");
    let client = TestClient::new(server::build_app::<Registered>(
        &ctx,
        verifier,
        Discovery::default(),
        None,
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

/// As CI and `make e2e` run it: with no `APP_AGENT_MODEL`, the server serves as usual with the
/// assistant off.
#[app_test_db::test(TEMPLATE)]
async fn without_a_model_the_assistant_is_off(pool: PgPool) {
    let verifier = JwtVerifier::new("test-secret");
    let bearer = format!(
        "Bearer {}",
        verifier.issue(NIL_ID, 60).expect("issue token")
    );
    let ctx = context(pool).await;
    let client = TestClient::new(server::build_app::<Registered>(
        &ctx,
        verifier,
        Discovery::default(),
        None,
        "http://localhost",
    ));

    let response = client
        .get(format!("{}/assistant/status", server::API_BASE))
        .header("Authorization", &bearer)
        .send()
        .await;

    response.assert_status_is_ok();
    response
        .assert_json(serde_json::json!({ "on": false }))
        .await;
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
    "list_grants",
    "grant_access",
    "revoke_grant",
    "get_access_catalog",
    "list_roles",
    "list_subjects",
    "create_role",
    "delete_role",
    "assign_role",
    "unassign_role",
    "invite_user",
    "list_invitations",
    "resend_invitation",
    "revoke_invitation",
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
    let ctx = context(pool).await;
    let api = TestApi::over(server::build_app::<Registered>(
        &ctx,
        verifier,
        Discovery::default(),
        None,
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
    let ctx = context(pool).await;
    let discovery = Discovery::new(
        PublicUrl::new(Some("https://app.example".into())),
        Some("https://issuer.example".into()),
    );
    let client = TestClient::new(server::build_app::<Registered>(
        &ctx,
        JwtVerifier::new("test-secret"),
        discovery,
        None,
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

#[app_test_db::test(TEMPLATE)]
async fn with_an_issuer_the_spa_is_told_how_to_sign_in(pool: PgPool) {
    let ctx = context(pool).await;
    let sign_in = SignIn {
        issuer: "https://issuer.example/".into(),
        client_id: Some("spa".into()),
        scopes: vec!["openid".into(), "offline_access".into()],
        extra_params: BTreeMap::from([("audience".into(), "https://api.example".into())]),
    };
    let client = TestClient::new(server::build_app::<Registered>(
        &ctx,
        JwtVerifier::new("test-secret"),
        Discovery::default(),
        Some(sign_in),
        "http://localhost",
    ));

    let response = client.get(sign_in::PATH).send().await;
    response.assert_status_is_ok();
    response
        .assert_json(serde_json::json!({
            "issuer": "https://issuer.example/",
            "clientId": "spa",
            "scopes": ["openid", "offline_access"],
            "extraParams": { "audience": "https://api.example" },
        }))
        .await;
}

#[app_test_db::test(TEMPLATE)]
async fn the_embedded_spa_reaches_its_own_origin_whatever_the_cors_origin(pool: PgPool) {
    let ctx = context(pool).await;
    let client = TestClient::new(server::build_app::<Registered>(
        &ctx,
        JwtVerifier::new("test-secret"),
        Discovery::default(),
        None,
        "http://localhost:5173",
    ));
    let from = |origin: &str, host: &str| {
        client
            .get("/health")
            .header("origin", origin)
            .header("host", host)
    };

    from("https://app.example", "app.example")
        .send()
        .await
        .assert_status_is_ok();
    from("http://127.0.0.1:8080", "127.0.0.1:8080")
        .send()
        .await
        .assert_status_is_ok();
    from("http://localhost:5173", "127.0.0.1:8080")
        .send()
        .await
        .assert_status_is_ok();
    from("https://elsewhere.example", "app.example")
        .send()
        .await
        .assert_status(poem::http::StatusCode::FORBIDDEN);
}

#[app_test_db::test(TEMPLATE)]
async fn without_an_issuer_there_is_no_sign_in_for_the_spa(pool: PgPool) {
    let ctx = context(pool).await;
    let client = TestClient::new(server::build_app::<Registered>(
        &ctx,
        JwtVerifier::new("test-secret"),
        Discovery::default(),
        None,
        "http://localhost",
    ));

    let response = client.get(sign_in::PATH).send().await;
    response.assert_status(poem::http::StatusCode::NOT_FOUND);
    let body = response.json().await;
    body.value().object().get("code").assert_string("not_found");
    body.value()
        .object()
        .get("message")
        .assert_string("no sign-in is configured: APP_JWT_ISSUER is unset");
}

#[test]
fn the_spas_sign_in_settings_are_no_operation() {
    let spec: Value = serde_json::from_str(&server::spec::<Registered>()).expect("spec is JSON");
    let paths = spec["paths"].as_object().expect("spec lists its paths");
    assert!(
        paths.keys().all(|path| !path.starts_with("/auth")),
        "{:?}",
        paths.keys()
    );
}

fn listed(list: &[(&str, &str)], id: &str) -> bool {
    list.iter().any(|(listed, _)| *listed == id)
}

#[test]
fn every_operation_is_guarded_unless_it_is_listed_as_public() {
    for (path, method, operation) in operations() {
        let id = operation["operationId"].as_str().unwrap_or_default();
        let guarded = operation["security"]
            .as_array()
            .is_some_and(|requirements| !requirements.is_empty());
        let public = listed(PUBLIC, id);
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
fn an_operation_open_to_every_signed_in_caller_is_guarded_and_performs_no_action() {
    let performs = server::operations::<Registered>();
    for (id, reason) in SIGNED_IN {
        assert!(
            !reason.is_empty(),
            "`{id}` is signed-in only without a reason"
        );
        assert!(
            !listed(PUBLIC, id),
            "`{id}` is listed in both `server::PUBLIC` and `server::SIGNED_IN`"
        );
        let operation = operations()
            .map(|(_, _, operation)| operation)
            .find(|operation| operation["operationId"] == *id)
            .unwrap_or_else(|| {
                panic!("`{id}` is listed in `server::SIGNED_IN` but there is no such operation")
            });
        assert!(
            operation["security"]
                .as_array()
                .is_some_and(|requirements| !requirements.is_empty()),
            "`{id}` is listed in `server::SIGNED_IN` but has no security requirement: take \
             `Caller`"
        );
        assert!(
            performs.iter().all(|p| p.operation != *id),
            "`{id}` is listed in `server::SIGNED_IN` and in its module's `OPERATIONS`: it is \
             one or the other"
        );
    }
}

#[test]
fn every_guarded_operation_names_an_action_its_modules_policies_have() {
    let policies = server::policies::<Registered>().expect("every module's policies load");
    let performs = server::operations::<Registered>();
    for (path, method, operation) in operations() {
        let id = operation["operationId"].as_str().unwrap_or_default();
        if listed(PUBLIC, id) || listed(SIGNED_IN, id) {
            continue;
        }
        let named: Vec<_> = performs.iter().filter(|p| p.operation == id).collect();
        assert_eq!(
            named.len(),
            1,
            "{method} {path} ({id}) is guarded, so its module's `OPERATIONS` names the action \
             it performs, once, unless it is listed in `server::SIGNED_IN` with why"
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
                .capability(PolicyFile::new("capability.yaml", "id: gadgets\nlabel: Gadgets\n"))
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

#[app_test_db::test(TEMPLATE)]
async fn a_request_is_one_span_with_its_operation_continuing_the_callers_trace(pool: PgPool) {
    const CALLERS_TRACE: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
    const CALLERS_SPAN: &str = "00f067aa0ba902b7";
    let verifier = JwtVerifier::new("test-secret");
    let bearer = format!(
        "Bearer {}",
        verifier.issue(NIL_ID, 60).expect("issue token")
    );
    let client = TestClient::new(server::build_app::<Registered>(
        &context(pool).await,
        verifier,
        Discovery::default(),
        None,
        "http://localhost",
    ));
    let traces = Traces::install();

    // An `access` operation, the one module `make init` leaves on the server.
    client
        .delete("/api/access/roles/nobody")
        .header("authorization", bearer)
        .header("x-request-id", "the-request")
        .header(
            "traceparent",
            format!("00-{CALLERS_TRACE}-{CALLERS_SPAN}-01"),
        )
        .send()
        .await
        .assert_status(poem::http::StatusCode::PRECONDITION_REQUIRED);

    let span = traces.span("DELETE /api/access/roles/{name}");
    let attribute = |key| Traces::attribute(&span, key);
    assert_eq!(attribute("operation_id").as_deref(), Some("delete_role"));
    assert_eq!(attribute("request_id").as_deref(), Some("the-request"));
    assert_eq!(attribute("user.id").as_deref(), Some(NIL_ID));
    assert_eq!(
        attribute("http.response.status_code").as_deref(),
        Some("428")
    );
    assert_eq!(span.span_context.trace_id().to_string(), CALLERS_TRACE);
    assert_eq!(span.parent_span_id.to_string(), CALLERS_SPAN);
    assert!(span.parent_span_is_remote);
}

#[app_test_db::test(TEMPLATE)]
async fn each_query_a_request_makes_is_a_child_span_with_its_statement_and_none_of_its_values(
    pool: PgPool,
) {
    const BOUND: &str = "a-value-only-ever-bound-7f3a9c";
    let verifier = JwtVerifier::new("test-secret");
    let bearer = format!(
        "Bearer {}",
        verifier.issue(NIL_ID, 60).expect("issue token")
    );
    let client = TestClient::new(server::build_app::<Registered>(
        &context(pool).await,
        verifier,
        Discovery::default(),
        None,
        "http://localhost",
    ));
    let traces = Traces::install();

    client
        .post("/api/access/roles")
        .header("authorization", bearer)
        .body_json(&serde_json::json!({ "name": BOUND, "description": BOUND }))
        .send()
        .await
        .assert_status_is_ok();

    let request = traces.span("POST /api/access/roles");
    let spans = traces.spans();
    let queries: Vec<_> = spans
        .iter()
        .filter(|span| Traces::attribute(span, "db.query.text").is_some())
        .collect();
    assert!(
        queries
            .iter()
            .any(|query| Traces::attribute(query, "db.query.text")
                .is_some_and(|text| text.contains("INSERT INTO access.role"))),
        "no span for the role's INSERT"
    );
    for query in &queries {
        assert_eq!(
            query.span_context.trace_id(),
            request.span_context.trace_id()
        );
        assert!(
            spans
                .iter()
                .any(|span| span.span_context.span_id() == query.parent_span_id),
            "{:?} isn't a child of a span the request made",
            query.name
        );
        assert_eq!(
            Traces::attribute(query, "db.system.name").as_deref(),
            Some("postgresql")
        );
    }
    for span in &spans {
        let attributes = span
            .attributes
            .iter()
            .chain(span.events.iter().flat_map(|event| &event.attributes));
        for attribute in attributes {
            assert!(
                !attribute.value.to_string().contains(BOUND),
                "span {:?} carries the bound value in {}",
                span.name,
                attribute.key
            );
        }
    }
}

#[app_test_db::test(TEMPLATE)]
async fn every_request_side_metric_is_recorded(pool: PgPool) {
    let metrics = Metrics::install();
    app_db::observe_pool(&pool);
    let verifier = JwtVerifier::new("test-secret");
    let token = verifier.issue(NIL_ID, 60).expect("issue token");
    let api = TestApi::over(server::build_app::<Registered>(
        &context(pool).await,
        verifier,
        Discovery::default(),
        None,
        "http://localhost",
    ));

    let create = |name: &str| {
        api.post("/api/access/roles")
            .bearer(&token)
            .idempotency_key("metrics-key")
            .json(&serde_json::json!({ "name": name, "description": "measured" }))
    };
    create("measured").answered().await;
    create("measured").answered().await;
    create("another").answered().await;
    let session = api.mcp_session::<Code>(Some(&token)).await.unwrap();
    session
        .call::<Value, Code>("list_roles", serde_json::json!({}))
        .await
        .unwrap();

    let names = metrics.names();
    for metric in [
        "http.server.request.duration",
        "mcp.server.tool.calls",
        "db.client.connection.count",
        "db.client.connection.max",
        "db.client.connection.wait_time",
        "idempotency.replays",
        "idempotency.reuse_refusals",
    ] {
        assert!(names.contains(metric), "{metric} isn't in {names:?}");
    }
}
