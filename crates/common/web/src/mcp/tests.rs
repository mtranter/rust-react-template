//! The bridge against a small API of its own: how arguments become requests,
//! how answers become results, and how sessions are bound to their opener.

use super::*;
use crate::testing::{McpSession, Refused, TestApi};
use crate::{ApiResult, JwtAuth, JwtVerifier, PublicUrl, render_error};
use app_error::AppError;
use poem::endpoint::BoxEndpoint;
use poem::test::TestClient;
use poem::{Route, http::StatusCode};
use poem_mcpserver::tool::Tools;
use poem_openapi::param::{Header, Path, Query};
use poem_openapi::payload::{Json, PlainText};
use poem_openapi::{ApiResponse, Object, OpenApi, OpenApiService};
use serde_json::{Value, json};

#[derive(Object)]
struct Thing {
    name: String,
}

#[derive(Object)]
#[oai(rename_all = "camelCase")]
struct Received {
    subject: String,
    method: String,
    target: String,
    id: String,
    if_match: Option<String>,
    trace_tag: Option<String>,
    thing: Thing,
}

#[derive(ApiResponse)]
enum Deleted {
    #[oai(status = 204)]
    Deleted,
}

#[derive(Default)]
struct Things;

#[OpenApi]
impl Things {
    /// Replace a thing.
    ///
    /// Answers what it received.
    #[oai(path = "/things/:id", method = "put", operation_id = "put_thing")]
    #[allow(clippy::too_many_arguments)]
    async fn put(
        &self,
        req: &poem::Request,
        auth: JwtAuth,
        id: Path<String>,
        #[oai(name = "tag")] _tags: Query<Option<Vec<String>>>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
        #[oai(name = "X-Trace-Tag")] trace_tag: Header<Option<String>>,
        body: Json<Thing>,
    ) -> Json<Received> {
        Json(Received {
            subject: auth.0.id().unwrap_or_default().to_owned(),
            method: req.method().to_string(),
            target: req.original_uri().to_string(),
            id: id.0,
            if_match: if_match.0,
            trace_tag: trace_tag.0,
            thing: body.0,
        })
    }

    /// Delete a thing.
    #[oai(path = "/things/:id", method = "delete", operation_id = "delete_thing")]
    async fn delete(&self, _auth: JwtAuth, id: Path<String>) -> ApiResult<Deleted> {
        if id.0 == "missing" {
            return Err(AppError::not_found("thing").into());
        }
        Ok(Deleted::Deleted)
    }
}

struct ThingTools;

impl Surface for ThingTools {
    type Api = Things;
    const BASE: &'static str = "/api";
    const NAME: &'static str = "things";
    const VERSION: &'static str = "1.0";
    const INSTRUCTIONS: &'static str = "Things.";
    const REST_ONLY: &'static [RestOnly] = &[];
}

#[derive(Default)]
struct Faulty;

#[OpenApi]
impl Faulty {
    /// Names its version twice.
    #[oai(path = "/twice", method = "patch", operation_id = "twice")]
    async fn twice(
        &self,
        #[oai(name = "If-Match")] _if_match: Header<Option<String>>,
        #[oai(name = "expected_version")] _version: Query<Option<String>>,
    ) -> PlainText<String> {
        PlainText(String::new())
    }

    /// Takes text.
    #[oai(path = "/text", method = "post", operation_id = "take_text")]
    async fn text(&self, _body: PlainText<String>) -> Json<Thing> {
        Json(Thing {
            name: String::new(),
        })
    }

    /// Takes text, but is kept off MCP.
    #[oai(path = "/kept", method = "post", operation_id = "kept")]
    async fn kept(&self, _body: PlainText<String>) -> Json<Thing> {
        Json(Thing {
            name: String::new(),
        })
    }

    #[oai(path = "/unsummarised", method = "get", operation_id = "unsummarised")]
    async fn unsummarised(&self) -> Json<Thing> {
        Json(Thing {
            name: String::new(),
        })
    }
}

struct FaultyTools;

impl Surface for FaultyTools {
    type Api = Faulty;
    const BASE: &'static str = "/api";
    const NAME: &'static str = "faulty";
    const VERSION: &'static str = "1.0";
    const INSTRUCTIONS: &'static str = "";
    const REST_ONLY: &'static [RestOnly] = &[
        RestOnly {
            operation_id: "kept",
            reason: "takes text",
        },
        RestOnly {
            operation_id: "gone",
            reason: "no longer exists",
        },
    ];
}

const ISSUER: &str = "https://issuer.example";

/// The app as the server mounts it: the API at `/api`, and MCP sending
/// through the same endpoint.
fn app(verifier: &JwtVerifier, discovery: Discovery) -> BoxEndpoint<'static> {
    let api = Arc::new(
        Route::new()
            .nest("/api", OpenApiService::new(Things, "things", "1.0"))
            .catch_all_error(render_error)
            .data(verifier.clone())
            .boxed(),
    );
    Route::new()
        .nest_no_strip("/api", api.clone())
        .at(PATH, endpoint::<ThingTools>(api, discovery.clone()))
        .at(METADATA_PATH, discovery.metadata())
        .at(format!("{METADATA_PATH}{PATH}"), discovery.metadata())
        .catch_all_error(render_error)
        .with(RequestIdMiddleware)
        .data(verifier.clone())
        .boxed()
}

struct Agent {
    api: TestApi,
    verifier: JwtVerifier,
}

impl Agent {
    fn new() -> Self {
        let verifier = JwtVerifier::new("secret");
        Self {
            api: TestApi::over(app(&verifier, Discovery::default())),
            verifier,
        }
    }

    fn token_of(&self, subject: &str) -> String {
        self.verifier.issue(subject, 60).unwrap()
    }

    async fn session(&self, subject: &str) -> McpSession<'_> {
        let token = self.token_of(subject);
        self.api.mcp_session::<Code>(Some(&token)).await.unwrap()
    }
}

#[derive(Debug, PartialEq)]
struct Code(String);

impl From<Refused> for Code {
    fn from(refused: Refused) -> Self {
        Self(refused.code)
    }
}

fn code(code: &str) -> Code {
    Code(code.to_owned())
}

fn put(session: &McpSession<'_>, arguments: Value) -> impl Future<Output = Result<Value, Code>> {
    session.call("put_thing", arguments)
}

#[tokio::test]
async fn each_operation_is_a_tool_described_and_typed_as_in_the_spec() {
    let agent = Agent::new();
    let session = agent.session("ada").await;

    let tools = session.tools::<Code>().await.unwrap();
    assert_eq!(
        tools.iter().map(|t| &t["name"]).collect::<Vec<_>>(),
        ["put_thing", "delete_thing"]
    );
    let put = &tools[0];
    assert_eq!(
        put["description"],
        "Replace a thing.\n\nAnswers what it received."
    );
    assert_eq!(tools[1]["description"], "Delete a thing.");

    let input = &put["inputSchema"];
    assert_eq!(input["type"], "object");
    assert_eq!(input["required"], json!(["id", "body"]));
    assert_eq!(input["additionalProperties"], false);
    let properties = input["properties"].as_object().unwrap();
    assert_eq!(
        properties.keys().collect::<Vec<_>>(),
        ["body", "expected_version", "id", "tag", "x_trace_tag"]
    );
    assert_eq!(properties["tag"]["type"], "array");
    assert!(
        properties["expected_version"]["description"]
            .as_str()
            .unwrap()
            .contains("precondition_failed")
    );
    assert_eq!(properties["body"]["$ref"], "#/$defs/Thing");
    assert_eq!(
        input["$defs"]["Thing"]["properties"]["name"]["type"],
        "string"
    );

    let output = &put["outputSchema"];
    assert_eq!(output["type"], "object");
    assert_eq!(output["properties"]["thing"]["$ref"], "#/$defs/Thing");
    assert!(output["$defs"]["Thing"].is_object());
    assert!(tools[1].get("outputSchema").is_none(), "a 204 has no body");
}

#[tokio::test]
async fn arguments_become_the_operations_method_path_query_headers_and_body() {
    let agent = Agent::new();
    let session = agent.session("ada").await;

    let received = put(
        &session,
        json!({
            "id": "a b/c",
            "tag": ["x", "y&z"],
            "expected_version": "3",
            "x_trace_tag": "traced",
            "body": { "name": "Widget" },
        }),
    )
    .await;

    assert_eq!(
        received,
        Ok(json!({
            "subject": "ada",
            "method": "PUT",
            "target": "/api/things/a%20b%2Fc?tag=x&tag=y%26z",
            "id": "a b/c",
            "ifMatch": "\"3\"",
            "traceTag": "traced",
            "thing": { "name": "Widget" },
        }))
    );
}

#[tokio::test]
async fn an_argument_the_operation_refuses_is_refused_as_the_operation_refuses_it() {
    let agent = Agent::new();
    let session = agent.session("ada").await;

    assert_eq!(
        put(&session, json!({ "id": "a", "body": { "name": ["n"] } })).await,
        Err(code("malformed_request"))
    );
    assert_eq!(
        put(&session, json!({ "id": "a" })).await,
        Err(code("unsupported_media_type"))
    );
}

#[tokio::test]
async fn an_argument_no_operation_takes_is_refused_before_any_request() {
    let agent = Agent::new();
    let session = agent.session("ada").await;

    assert_eq!(
        put(
            &session,
            json!({ "id": "a", "body": { "name": "n" }, "colour": "red" })
        )
        .await,
        Err(code("malformed_request"))
    );
}

#[tokio::test]
async fn an_operation_answering_no_body_answers_an_empty_result() {
    let agent = Agent::new();
    let session = agent.session("ada").await;

    assert_eq!(
        session
            .call::<Value, Code>("delete_thing", json!({ "id": "a" }))
            .await,
        Ok(Value::Null)
    );
}

#[tokio::test]
async fn a_refusal_answers_the_operations_error_body() {
    let agent = Agent::new();
    let session = agent.session("ada").await;

    assert_eq!(
        session
            .call::<Value, Code>("delete_thing", json!({ "id": "missing" }))
            .await,
        Err(code("not_found"))
    );
}

#[test]
fn an_operation_that_cant_be_a_tool_is_a_problem_unless_kept_off_mcp() {
    let mut found = problems::<FaultyTools>();
    found.sort();
    assert_eq!(
        found,
        [
            "REST_ONLY names `gone`, which no operation is",
            "take_text (POST /text) takes a `text/plain; charset=utf-8` body, not JSON: list it in \
             REST_ONLY",
            "twice (PATCH /twice) takes two arguments named `expected_version`",
            "unsummarised (GET /unsummarised) has no summary",
        ]
    );
    assert!(OpenApiTools::<FaultyTools>::list().is_empty());
    assert!(problems::<ThingTools>().is_empty());
}

#[tokio::test]
async fn a_sessions_tools_act_as_the_subject_whose_token_opened_it() {
    let agent = Agent::new();
    let session = agent.session("ada").await;

    let received = put(&session, json!({ "id": "a", "body": { "name": "n" } }))
        .await
        .unwrap();
    assert_eq!(received["subject"], "ada");
}

#[tokio::test]
async fn a_session_presents_the_token_its_latest_request_did() {
    let agent = Agent::new();
    let session = agent.session("ada").await;
    let renewed = agent.verifier.issue("ada", 3600).unwrap();

    let received = put(
        &session.presenting(Some(&renewed)),
        json!({ "id": "a", "body": { "name": "n" } }),
    )
    .await;
    assert_eq!(received.unwrap()["subject"], "ada");
}

// Else another subject's changes would be made as the opener.
#[tokio::test]
async fn another_subjects_token_finds_no_session() {
    let agent = Agent::new();
    let session = agent.session("ada").await;
    let grace = agent.token_of("grace");

    let call = json!({ "id": "a", "body": { "name": "n" } });
    assert_eq!(
        put(&session.presenting(Some(&grace)), call.clone()).await,
        Err(code("not_found"))
    );
    assert_eq!(put(&session, call).await.unwrap()["subject"], "ada");
}

#[tokio::test]
async fn a_request_without_a_valid_token_is_refused() {
    let agent = Agent::new();

    assert_eq!(
        agent.api.mcp_session::<Code>(None).await.err(),
        Some(code("unauthorized"))
    );
    assert_eq!(
        agent.api.mcp_session::<Code>(Some("forged")).await.err(),
        Some(code("unauthorized"))
    );
    let session = agent.session("ada").await;
    assert_eq!(
        session.presenting(None).tool_names::<Code>().await,
        Err(code("unauthorized"))
    );
}

#[tokio::test]
async fn a_closed_session_is_gone() {
    let agent = Agent::new();
    let session = agent.session("ada").await;

    session.close::<Code>().await.unwrap();
    assert_eq!(session.tool_names::<Code>().await, Err(code("not_found")));
}

fn client(discovery: Discovery) -> TestClient<BoxEndpoint<'static>> {
    TestClient::new(app(&JwtVerifier::new("secret"), discovery))
}

fn with_issuer() -> Discovery {
    Discovery::new(
        PublicUrl::new(Some("https://app.example/".into())),
        Some(ISSUER.into()),
    )
}

#[tokio::test]
async fn with_an_issuer_a_tokenless_request_is_challenged_to_sign_in_there() {
    let response = client(with_issuer()).post(PATH).send().await;

    response.assert_status(StatusCode::UNAUTHORIZED);
    response.assert_header(
        "www-authenticate",
        r#"Bearer resource_metadata="https://app.example/.well-known/oauth-protected-resource/mcp""#,
    );
    let body = response.json().await;
    body.value()
        .object()
        .get("code")
        .assert_string("unauthorized");
}

#[tokio::test]
async fn without_an_issuer_a_tokenless_request_is_refused_unchallenged() {
    let response = client(Discovery::default()).post(PATH).send().await;

    response.assert_status(StatusCode::UNAUTHORIZED);
    response.assert_header_is_not_exist("www-authenticate");
}

#[tokio::test]
async fn with_an_issuer_the_protected_resource_metadata_is_served_on_both_paths() {
    let client = client(with_issuer());
    for path in [METADATA_PATH.to_owned(), format!("{METADATA_PATH}{PATH}")] {
        let response = client.get(&path).send().await;
        response.assert_status_is_ok();
        response
            .assert_json(json!({
                "resource": "https://app.example/mcp",
                "authorization_servers": [ISSUER],
                "bearer_methods_supported": ["header"],
            }))
            .await;
    }
}

#[tokio::test]
async fn without_an_issuer_there_is_no_protected_resource_metadata() {
    let client = client(Discovery::default());
    for path in [METADATA_PATH.to_owned(), format!("{METADATA_PATH}{PATH}")] {
        client
            .get(&path)
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn a_tool_call_is_a_span_named_by_the_tool_parent_of_its_operations_request() {
    let agent = Agent::new();
    let session = agent.session("ada").await;
    let traces = crate::testing::Traces::install();

    put(&session, json!({ "id": "a", "body": { "name": "n" } }))
        .await
        .unwrap();
    assert_eq!(
        session
            .call::<Value, Code>("delete_thing", json!({ "id": "missing" }))
            .await,
        Err(code("not_found"))
    );

    for (tool, request, is_error) in [
        ("put_thing", "PUT /api/things/{id}", "false"),
        ("delete_thing", "DELETE /api/things/{id}", "true"),
    ] {
        let call = traces.span(tool);
        assert_eq!(
            crate::testing::Traces::attribute(&call, "is_error").as_deref(),
            Some(is_error),
            "{tool}"
        );
        let request = traces.span(request);
        assert_eq!(
            request.parent_span_id,
            call.span_context.span_id(),
            "{tool}"
        );
        assert_eq!(
            request.span_context.trace_id(),
            call.span_context.trace_id(),
            "{tool}"
        );
    }
    let failed = traces.span("delete_thing");
    assert!(matches!(
        failed.status,
        opentelemetry::trace::Status::Error { .. }
    ));
}

#[tokio::test]
async fn a_tool_call_refused_before_any_request_is_an_error_span_with_no_request() {
    let agent = Agent::new();
    let session = agent.session("ada").await;
    let traces = crate::testing::Traces::install();

    assert_eq!(
        put(
            &session,
            json!({ "id": "a", "body": { "name": "n" }, "colour": "red" })
        )
        .await,
        Err(code("malformed_request"))
    );

    let call = traces.span("put_thing");
    assert!(matches!(
        call.status,
        opentelemetry::trace::Status::Error { .. }
    ));
    assert_eq!(crate::testing::Traces::attribute(&call, "is_error"), None);
}

#[test]
fn an_agent_tool_says_its_path_where_each_argument_goes_and_its_fields_in_order() {
    let toolbox = Toolbox::new::<ThingTools>();
    let put = toolbox
        .tools()
        .iter()
        .find(|tool| tool.name == "put_thing")
        .expect("put_thing is a tool");

    assert_eq!(put.path, "/things/{id}");
    let sent: Vec<_> = put
        .arguments
        .iter()
        .map(|argument| (argument.name.as_str(), argument.sent))
        .collect();
    assert_eq!(
        sent,
        vec![
            ("id", ArgumentIn::Path),
            ("tag", ArgumentIn::Query),
            ("expected_version", ArgumentIn::Header),
            ("x_trace_tag", ArgumentIn::Header),
            ("body", ArgumentIn::Body),
        ]
    );
    let fields: Vec<_> = put
        .fields
        .iter()
        .map(|field| {
            let mut at = vec![field.argument.as_str()];
            at.extend(field.properties.iter().map(|p| p.name.as_str()));
            at.join(".")
        })
        .collect();
    assert_eq!(
        fields,
        vec!["id", "tag", "expected_version", "x_trace_tag", "body.name"]
    );
}
