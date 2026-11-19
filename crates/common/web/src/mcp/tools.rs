//! A tool per operation of an [`OpenApi`], described by the operation's meta
//! and called by sending the operation's request through the REST endpoint.

use super::schema::{Definitions, json_schema};
use super::sessions::Presented;
use crate::metrics::ToolCalls;
use crate::render_error;
use crate::request_id::current_request_id;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use poem::endpoint::BoxEndpoint;
use poem::http::{Method, header, uri::Scheme};
use poem::web::{LocalAddr, RemoteAddr};
use poem::{Endpoint, Request, RequestParts, Response};
use poem_mcpserver::protocol::content::Content;
use poem_mcpserver::protocol::rpc::RpcError;
use poem_mcpserver::protocol::tool::{Tool, ToolsCallResponse};
use poem_mcpserver::tool::Tools;
use poem_openapi::OpenApi;
use poem_openapi::registry::{MetaOperation, MetaParamIn, Registry};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex, PoisonError};

/// What the MCP endpoint offers: a tool per operation of `Api`, but those
/// [`REST_ONLY`](Self::REST_ONLY). Type-level, since `poem-mcpserver` lists
/// tools and instructions without a value.
pub trait Surface: Send + Sync + 'static {
    /// The API whose operations are tools, as it is served.
    type Api: OpenApi;
    /// Where `Api` is served, which each operation's path is under.
    const BASE: &'static str;
    /// The server's name and version, as an MCP client is told them.
    const NAME: &'static str;
    const VERSION: &'static str;
    /// What an agent should know to use the tools.
    const INSTRUCTIONS: &'static str;
    /// Operations deliberately offered only over REST.
    const REST_ONLY: &'static [RestOnly];
}

/// An operation kept off MCP, and why.
pub struct RestOnly {
    pub operation_id: &'static str,
    pub reason: &'static str,
}

/// What [`Surface::REST_ONLY`] can't leave out silently: an operation that
/// can't be a tool, or two arguments of one tool with one name. A tool
/// listing leaves each such operation out; a test asserts there are none.
pub fn problems<S: Surface>() -> Vec<String> {
    described::<S>().1
}

/// The description of the If-Match header as the argument naming the version.
const EXPECTED_VERSION: &str = "The `version` you last read of what this changes. A stale one is \
    refused as `precondition_failed` and changes nothing; leaving it out is refused as \
    `precondition_required`.";

/// RFC 3986's unreserved characters stay; everything else is encoded, so a
/// value is one path segment or one query value whatever it holds.
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// The tools of `S`, for one session: each call is sent through `api`,
/// presenting the session's current token.
pub struct OpenApiTools<S> {
    api: Arc<BoxEndpoint<'static, Response>>,
    presented: Presented,
    calls: ToolCalls,
    surface: PhantomData<fn() -> S>,
}

impl<S> OpenApiTools<S> {
    pub(super) fn new(api: Arc<BoxEndpoint<'static, Response>>, presented: Presented) -> Self {
        Self {
            api,
            presented,
            calls: ToolCalls::new(),
            surface: PhantomData,
        }
    }
}

impl<S: Surface> Tools for OpenApiTools<S> {
    fn instructions() -> &'static str {
        S::INSTRUCTIONS
    }

    fn list() -> Vec<Tool> {
        described::<S>()
            .0
            .into_iter()
            .map(|operation| Tool {
                name: operation.name,
                description: operation.description,
                input_schema: operation.input_schema,
                output_schema: operation.output_schema,
            })
            .collect()
    }

    async fn call(&mut self, name: &str, arguments: Value) -> Result<ToolsCallResponse, RpcError> {
        let operation = described::<S>()
            .0
            .into_iter()
            .find(|operation| operation.name == name)
            .ok_or_else(|| RpcError::method_not_found(format!("no tool is named `{name}`")))?;
        let request = operation
            .request(S::BASE, arguments, &self.presented.token())
            .map_err(|err| {
                self.calls.add(operation.name, true);
                RpcError::invalid_params(err)
            })?;
        let response = match self.api.call(request).await {
            Ok(response) => response,
            Err(error) => render_error(error).await,
        };
        let answered = answer(response, operation.output_schema.is_some()).await;
        self.calls.add(operation.name, answered.is_error);
        Ok(answered)
    }
}

/// An operation as a tool.
struct Described {
    name: &'static str,
    description: &'static str,
    input_schema: Value,
    output_schema: Option<Value>,
    method: Method,
    path: String,
    arguments: Vec<Argument>,
}

struct Argument {
    name: String,
    sent: Sent,
}

/// Where an argument goes in the operation's request.
enum Sent {
    Path(String),
    Query(String),
    Header(String),
    Body,
}

/// The tools of `S`, and what keeps any other operation from being one.
fn described<S: Surface>() -> (Vec<Described>, Vec<String>) {
    let mut registry = Registry::new();
    S::Api::register(&mut registry);
    let definitions = Definitions::of(&registry);
    let mut tools = Vec::new();
    let mut problems = Vec::new();
    let mut ids = BTreeSet::new();
    for path in S::Api::meta().into_iter().flat_map(|api| api.paths) {
        for operation in &path.operations {
            let at = format!("{} {}", operation.method, path.path);
            let Some(id) = operation.operation_id else {
                problems.push(format!("{at} has no operation_id"));
                continue;
            };
            ids.insert(id);
            if S::REST_ONLY.iter().any(|kept| kept.operation_id == id) {
                continue;
            }
            match describe(id, operation, &path.path, &definitions) {
                Ok(tool) => tools.push(tool),
                Err(problem) => problems.push(format!("{id} ({at}) {problem}")),
            }
        }
    }
    for kept in S::REST_ONLY {
        if !ids.contains(kept.operation_id) {
            problems.push(format!(
                "REST_ONLY names `{}`, which no operation is",
                kept.operation_id
            ));
        }
    }
    (tools, problems)
}

fn describe(
    name: &'static str,
    operation: &MetaOperation,
    path: &str,
    definitions: &Definitions,
) -> Result<Described, String> {
    let description = match (operation.summary, operation.description) {
        (Some(summary), Some(description)) => intern(format!("{summary}\n\n{description}")),
        (Some(text), None) | (None, Some(text)) => text,
        (None, None) => return Err("has no summary".into()),
    };

    let mut arguments = Vec::new();
    let mut properties = Map::new();
    let mut required = Vec::new();
    for param in &operation.params {
        let (argument, sent) = match param.in_type {
            MetaParamIn::Path => (param.name.clone(), Sent::Path(param.name.clone())),
            MetaParamIn::Query => (param.name.clone(), Sent::Query(param.name.clone())),
            MetaParamIn::Header if param.name.eq_ignore_ascii_case("If-Match") => (
                "expected_version".to_owned(),
                Sent::Header(param.name.clone()),
            ),
            MetaParamIn::Header => (snake_case(&param.name), Sent::Header(param.name.clone())),
            _ => {
                return Err(format!(
                    "takes the cookie `{}`: list it in REST_ONLY",
                    param.name
                ));
            }
        };
        let mut schema = json_schema(&param.schema);
        let about = match &sent {
            Sent::Header(header) if header.eq_ignore_ascii_case("If-Match") => {
                Some(EXPECTED_VERSION.to_owned())
            }
            _ => param.description.clone(),
        };
        if let (Some(about), Value::Object(object)) = (about, &mut schema) {
            object.insert("description".into(), about.into());
        }
        if param.required || matches!(sent, Sent::Path(_)) {
            required.push(Value::from(argument.clone()));
        }
        properties.insert(argument.clone(), schema);
        arguments.push(Argument {
            name: argument,
            sent,
        });
    }
    if let Some(request) = &operation.request {
        let [content] = request.content.as_slice() else {
            return Err("takes several kinds of body: list it in REST_ONLY".into());
        };
        if !is_json(content.content_type) {
            return Err(format!(
                "takes a `{}` body, not JSON: list it in REST_ONLY",
                content.content_type
            ));
        }
        if request.required {
            required.push("body".into());
        }
        properties.insert("body".into(), json_schema(&content.schema));
        arguments.push(Argument {
            name: "body".into(),
            sent: Sent::Body,
        });
    }
    let mut names = BTreeSet::new();
    if let Some(twice) = arguments.iter().find(|a| !names.insert(&a.name)) {
        return Err(format!("takes two arguments named `{}`", twice.name));
    }

    let input_schema = definitions.standalone(json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    }));

    Ok(Described {
        name,
        description,
        input_schema,
        output_schema: output_schema(operation, definitions)?,
        method: operation.method.clone(),
        path: path.to_owned(),
        arguments,
    })
}

/// The schema of the operation's successful answer, if it has a body MCP can
/// carry as structured content: an object. Any other JSON is answered as
/// text alone.
fn output_schema(
    operation: &MetaOperation,
    definitions: &Definitions,
) -> Result<Option<Value>, String> {
    let Some(success) = operation.responses.responses.iter().find(|response| {
        response
            .status
            .is_some_and(|status| (200..300).contains(&status))
            || response.status_range.as_deref() == Some("2XX")
    }) else {
        return Ok(None);
    };
    let Some(content) = success.content.first() else {
        return Ok(None);
    };
    if !is_json(content.content_type) {
        return Err(format!(
            "answers `{}`, not JSON: list it in REST_ONLY",
            content.content_type
        ));
    }
    let schema = definitions.resolved(json_schema(&content.schema));
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return Ok(None);
    }
    Ok(Some(definitions.standalone(schema)))
}

impl Described {
    /// The operation's request, as `arguments` say to make it.
    fn request(&self, base: &str, arguments: Value, token: &str) -> Result<Request, String> {
        let mut arguments = match arguments {
            Value::Object(arguments) => arguments,
            Value::Null => Map::new(),
            _ => return Err("arguments are an object".into()),
        };
        if let Some(unknown) = arguments
            .keys()
            .find(|key| self.arguments.iter().all(|a| &a.name != *key))
        {
            return Err(format!("`{}` takes no argument `{unknown}`", self.name));
        }

        let mut path = self.path.clone();
        let mut query = Vec::new();
        let mut request = Request::builder()
            .method(self.method.clone())
            .header(header::AUTHORIZATION, format!("Bearer {token}"));
        if let Some(id) = current_request_id() {
            request = request.header("x-request-id", id);
        }
        let mut body = None;
        for argument in &self.arguments {
            let Some(value) = arguments.remove(&argument.name).filter(|v| !v.is_null()) else {
                if let Sent::Path(_) = argument.sent {
                    return Err(format!("`{}` needs `{}`", self.name, argument.name));
                }
                continue;
            };
            match &argument.sent {
                Sent::Path(name) => {
                    let segment = utf8_percent_encode(&text(&value), COMPONENT).to_string();
                    path = path.replace(&format!("{{{name}}}"), &segment);
                }
                Sent::Query(name) => {
                    let values = match value {
                        Value::Array(values) => values,
                        value => vec![value],
                    };
                    query.extend(values.iter().map(|value| {
                        format!(
                            "{}={}",
                            utf8_percent_encode(name, COMPONENT),
                            utf8_percent_encode(&text(value), COMPONENT)
                        )
                    }));
                }
                Sent::Header(name) if name.eq_ignore_ascii_case("If-Match") => {
                    let version = text(&value);
                    request =
                        request.header(name.as_str(), format!("\"{}\"", version.trim_matches('"')));
                }
                Sent::Header(name) => request = request.header(name.as_str(), text(&value)),
                Sent::Body => body = Some(value),
            }
        }

        let target = if query.is_empty() {
            format!("{base}{path}")
        } else {
            format!("{base}{path}?{}", query.join("&"))
        };
        request = request.uri(
            target
                .parse()
                .map_err(|_| format!("`{target}` is not a request target"))?,
        );
        let request = match body {
            Some(body) => request
                .content_type("application/json")
                .body(body.to_string()),
            None => request.finish(),
        };
        Ok(as_received(request))
    }
}

/// `request` as a server receives one: built, it has no `original_uri`, the
/// target before any `nest` strips its prefix.
fn as_received(request: Request) -> Request {
    let (parts, body) = request.into_parts();
    let (mut head, ()) = poem::http::Request::new(()).into_parts();
    head.method = parts.method;
    head.uri = parts.uri;
    head.version = parts.version;
    head.headers = parts.headers;
    head.extensions = parts.extensions;
    let parts = RequestParts::from((
        head,
        LocalAddr::default(),
        RemoteAddr::default(),
        Scheme::HTTP,
    ));
    Request::from_parts(parts, body)
}

/// What a tool answers for the operation's `response`: a body as text, and
/// as structured content where the tool declares a schema for it; a refusal
/// as its error body, flagged as an error.
async fn answer(response: Response, structured: bool) -> ToolsCallResponse {
    let succeeded = response.status().is_success();
    let body = response.into_body().into_vec().await.unwrap_or_default();
    let structured_content = (succeeded && structured)
        .then(|| serde_json::from_slice(&body).ok())
        .flatten();
    ToolsCallResponse {
        content: vec![Content::Text {
            text: String::from_utf8_lossy(&body).into_owned(),
        }],
        structured_content,
        is_error: !succeeded,
    }
}

/// A scalar argument as it is written in a path, query or header.
fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn is_json(content_type: &str) -> bool {
    content_type
        .split(';')
        .next()
        .is_some_and(|essence| essence.trim().eq_ignore_ascii_case("application/json"))
}

/// `Idempotency-Key` → `idempotency_key`.
fn snake_case(header: &str) -> String {
    header.replace('-', "_").to_ascii_lowercase()
}

/// `text`, kept for the life of the process: a tool's description is
/// `&'static`. The set of descriptions is finite, so this leaks each once.
fn intern(text: String) -> &'static str {
    static INTERNED: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    let mut interned = INTERNED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(kept) = interned.get(text.as_str()) {
        return kept;
    }
    let kept: &'static str = Box::leak(text.into_boxed_str());
    interned.insert(kept);
    kept
}
