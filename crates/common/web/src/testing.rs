//! The transport half of a module's test driver (ADR 0004), behind the `test`
//! feature, which only dev-dependencies enable. The driver keeps what is its
//! module's own: domain verbs, JSON newtypes built from aggregates, and a
//! `Refusal` declaring only the codes the module produces.

mod traces;

pub use traces::Traces;

use crate::{IDEMPOTENCY_KEY, Idempotency, RequestIdMiddleware, TraceRequests, mcp, render_error};
use poem::endpoint::BoxEndpoint;
use poem::test::{TestClient, TestRequestBuilder};
use poem::{Endpoint, EndpointExt, Route};
use poem_openapi::{OpenApi, OpenApiService};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::fmt::Display;
use std::ops::Deref;

pub struct TestApi {
    client: TestClient<BoxEndpoint<'static>>,
}

impl TestApi {
    /// Mounts `endpoints` under `/api` as the server does: with its error
    /// rendering, `idempotency`, request ids and spans; `wire` adds the data they read
    /// (a provider, a verifier) outermost, as the server does.
    pub fn new<T, E>(
        endpoints: T,
        idempotency: Idempotency,
        wire: impl FnOnce(BoxEndpoint<'static>) -> E,
    ) -> Self
    where
        T: OpenApi + 'static,
        E: Endpoint + 'static,
    {
        let api = OpenApiService::new(endpoints, "test", "1.0");
        let app = Route::new()
            .nest("/api", api)
            .catch_all_error(render_error)
            .with(idempotency)
            .with(TraceRequests::of::<T>("/api"))
            .with(RequestIdMiddleware)
            .boxed();
        Self::over(wire(app))
    }

    /// Over a whole app that already renders errors and stamps request ids, as
    /// the composed server's `build_app` does.
    pub fn over<E>(app: E) -> Self
    where
        E: Endpoint + 'static,
    {
        Self {
            client: TestClient::new(app.map_to_response().boxed()),
        }
    }

    pub fn get(&self, uri: impl Into<String>) -> Call<'_> {
        Call(self.client.get(uri))
    }

    pub fn post(&self, uri: impl Into<String>) -> Call<'_> {
        Call(self.client.post(uri))
    }

    pub fn put(&self, uri: impl Into<String>) -> Call<'_> {
        Call(self.client.put(uri))
    }

    pub fn patch(&self, uri: impl Into<String>) -> Call<'_> {
        Call(self.client.patch(uri))
    }

    pub fn delete(&self, uri: impl Into<String>) -> Call<'_> {
        Call(self.client.delete(uri))
    }
}

/// An AI agent's session over MCP's streamable HTTP transport, as
/// [`TestApi::mcp_session`] opens it.
pub struct McpSession<'a> {
    api: &'a TestApi,
    id: String,
    token: Option<String>,
}

impl TestApi {
    /// Opens an MCP session as an agent presenting `token`, as a stock MCP
    /// client would: it initializes, then says it has. Needs an app serving
    /// MCP, as the composed server's does.
    pub async fn mcp_session<R: From<Refused>>(
        &self,
        token: Option<&str>,
    ) -> Result<McpSession<'_>, R> {
        let response = mcp_call(self, token, None)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "test-agent", "version": "1.0" },
                },
            }))
            .0
            .send()
            .await
            .0;
        if !response.status().is_success() {
            return Err(refused(response).await.into());
        }
        let id = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .expect("an opened session is named");
        let session = McpSession {
            api: self,
            id,
            token: token.map(str::to_owned),
        };
        session
            .send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .await
            .map_err(R::from)?;
        Ok(session)
    }
}

impl<'a> McpSession<'a> {
    /// This session, its requests presenting `token` instead, or no token.
    pub fn presenting(&self, token: Option<&str>) -> McpSession<'a> {
        McpSession {
            api: self.api,
            id: self.id.clone(),
            token: token.map(str::to_owned),
        }
    }

    /// Ends the session, as a stock MCP client does when it is done with it.
    pub async fn close<R: From<Refused>>(&self) -> Result<(), R> {
        let call = self
            .api
            .delete(mcp::PATH)
            .header("Mcp-Session-Id", &self.id);
        let call = match &self.token {
            Some(token) => call.bearer(token),
            None => call,
        };
        let response = call.0.send().await.0;
        if !response.status().is_success() {
            return Err(refused(response).await.into());
        }
        Ok(())
    }

    /// The tools the session offers, each as `tools/list` describes it.
    pub async fn tools<R: From<Refused>>(&self) -> Result<Vec<Value>, R> {
        let result = self.request("tools/list", json!({})).await?;
        Ok(result["tools"]
            .as_array()
            .expect("tools are listed")
            .clone())
    }

    /// The names of the tools the session offers.
    pub async fn tool_names<R: From<Refused>>(&self) -> Result<Vec<String>, R> {
        Ok(self
            .tools()
            .await?
            .iter()
            .map(|tool| tool["name"].as_str().expect("a tool is named").to_owned())
            .collect())
    }

    /// Calls `tool` with `arguments`: its structured result as `T` (`null`
    /// for an empty one), or the refusal its error result carries, checked to
    /// be in the one error shape.
    pub async fn call<T: DeserializeOwned, R: From<Refused>>(
        &self,
        tool: &str,
        arguments: Value,
    ) -> Result<T, R> {
        let result = self
            .request(
                "tools/call",
                json!({ "name": tool, "arguments": arguments }),
            )
            .await?;
        let text = result["content"][0]["text"]
            .as_str()
            .expect("a tool answers in text");
        let body: Value = if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(text).expect("a tool answers JSON")
        };
        if result["isError"] == json!(true) {
            return Err(refused_body(body, None).into());
        }
        if let Some(structured) = result.get("structuredContent") {
            assert_eq!(structured, &body, "a tool's text is its structured content");
        }
        Ok(serde_json::from_value(body).expect("the tool's result has the expected shape"))
    }

    async fn request<R: From<Refused>>(&self, method: &str, params: Value) -> Result<Value, R> {
        let mut answers = self
            .send(&json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }))
            .await?;
        let answer = answers.pop().expect("a request is answered");
        if let Some(error) = answer.get("error") {
            // JSON-RPC's invalid params: arguments the bridge couldn't make a
            // request of, so no operation answered with an `ErrorBody`.
            assert_eq!(error["code"], -32602, "{error}");
            assert!(
                !error["message"].as_str().unwrap_or_default().is_empty(),
                "a refusal explains itself"
            );
            return Err(R::from(Refused {
                code: "malformed_request".into(),
                paths: Vec::new(),
            }));
        }
        Ok(answer["result"].clone())
    }

    async fn send(&self, message: &Value) -> Result<Vec<Value>, Refused> {
        let response = mcp_call(self.api, self.token.as_deref(), Some(&self.id))
            .json(message)
            .0
            .send()
            .await
            .0;
        if !response.status().is_success() {
            return Err(refused(response).await);
        }
        let body = response
            .into_body()
            .into_bytes()
            .await
            .expect("read the body");
        // A notification is answered with an empty batch.
        Ok(serde_json::from_slice(&body).unwrap_or_default())
    }
}

fn mcp_call<'a>(api: &'a TestApi, token: Option<&str>, session: Option<&str>) -> Call<'a> {
    let call = api.post(mcp::PATH).header("Accept", "application/json");
    let call = match token {
        Some(token) => call.bearer(token),
        None => call,
    };
    match session {
        Some(id) => call.header("Mcp-Session-Id", id),
        None => call,
    }
}

pub struct Call<'a>(TestRequestBuilder<'a, BoxEndpoint<'static>>);

impl Call<'_> {
    pub fn json(self, body: &Value) -> Self {
        Self(self.0.body_json(body))
    }

    pub fn query(self, name: &str, value: impl Display) -> Self {
        Self(self.0.query(name, &value.to_string()))
    }

    pub fn queries(self, pairs: &[(&str, String)]) -> Self {
        pairs
            .iter()
            .fold(self, |call, (name, value)| call.query(name, value))
    }

    pub fn header(self, name: &'static str, value: impl Display) -> Self {
        Self(self.0.header(name, value.to_string()))
    }

    pub fn if_match(self, version: i64) -> Self {
        self.header("If-Match", format!("\"{version}\""))
    }

    pub fn idempotency_key(self, key: impl Display) -> Self {
        Self(self.0.header(IDEMPOTENCY_KEY, key.to_string()))
    }

    pub fn bearer(self, token: impl Display) -> Self {
        self.header("Authorization", format!("Bearer {token}"))
    }

    /// The body as `T`, or the refusal as the driver's `R`. Panics on a refusal
    /// without the one error shape, whatever raised it.
    pub async fn send<T: DeserializeOwned, R: From<Refused>>(self) -> Result<T, R> {
        let response = self.0.send().await.0;
        if response.status().is_success() {
            return Ok(response
                .into_body()
                .into_json()
                .await
                .expect("the response body has the expected shape"));
        }
        Err(refused(response).await.into())
    }

    /// The whole answer, success or refusal, for comparing a retry's with the
    /// original's.
    pub async fn answered(self) -> Answered {
        let response = self.0.send().await.0;
        let status = response.status().as_u16();
        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let body = response
            .into_body()
            .into_bytes()
            .await
            .expect("read the body");
        Answered {
            status,
            etag,
            body: if body.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&body).expect("the body is JSON")
            },
        }
    }

    /// For an operation that answers with no body, like a delete.
    pub async fn send_empty<R: From<Refused>>(self) -> Result<(), R> {
        let response = self.0.send().await.0;
        if response.status().is_success() {
            let body = response
                .into_body()
                .into_bytes()
                .await
                .expect("read the body");
            assert!(body.is_empty(), "the response has no body");
            return Ok(());
        }
        Err(refused(response).await.into())
    }
}

async fn refused(response: poem::Response) -> Refused {
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .expect("a refusal carries its request id");
    let body = response
        .into_body()
        .into_json()
        .await
        .expect("a refusal has the error shape");
    refused_body(body, Some(&request_id))
}

/// `body` checked to be in the one error shape, naming `request_id` if given.
fn refused_body(body: Value, request_id: Option<&str>) -> Refused {
    let error: ErrorShape = serde_json::from_value(body).expect("a refusal has the error shape");
    if let Some(request_id) = request_id {
        assert_eq!(
            error.request_id, request_id,
            "the body names the request id"
        );
    }
    assert!(!error.request_id.is_empty(), "a refusal names its request");
    if let Some(trace_id) = &error.trace_id {
        assert!(
            trace_id.len() == 32 && trace_id.bytes().all(|b| b.is_ascii_hexdigit()),
            "a trace id is W3C's 32 hex digits, not {trace_id:?}"
        );
    }
    assert!(!error.message.is_empty(), "a refusal explains itself");
    assert!(error.fields.iter().all(|f| !f.message.is_empty()));
    assert!(
        error.fields.is_empty() || error.code == "validation_error",
        "only a validation error names fields"
    );
    Refused {
        code: error.code,
        paths: error.fields.into_iter().map(|f| f.path).collect(),
    }
}

/// An answer as a client sees it, minus its request id, which differs per
/// request even when the answer is replayed.
#[derive(Debug, PartialEq)]
pub struct Answered {
    pub status: u16,
    pub etag: Option<String>,
    pub body: Value,
}

impl Answered {
    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_value(self.body.clone()).expect("the body has the expected shape")
    }

    /// The body as `T`, or the refusal as the driver's `R`, as [`Call::send`] reads it.
    pub fn outcome<T: DeserializeOwned, R: From<Refused>>(&self) -> Result<T, R> {
        if (200..300).contains(&self.status) {
            return Ok(self.json());
        }
        let error: ErrorShape =
            serde_json::from_value(self.body.clone()).expect("a refusal has the error shape");
        Err(Refused {
            code: error.code,
            paths: error.fields.into_iter().map(|f| f.path).collect(),
        }
        .into())
    }
}

/// A checked refusal: the driver maps `code` onto its own `Refusal`.
#[derive(Debug)]
pub struct Refused {
    pub code: String,
    /// The inputs at fault; only a `validation_error` names any.
    pub paths: Vec<String>,
}

/// Parsed independently of [`crate::ErrorBody`], as a client would, so a change to
/// the wire shape fails here.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ErrorShape {
    code: String,
    message: String,
    #[serde(default)]
    fields: Vec<FieldShape>,
    request_id: String,
    trace_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldShape {
    path: String,
    message: String,
}

/// A page of a list response (`ListResult`), its items read through the driver's
/// JSON newtype.
#[derive(Debug, Deserialize)]
pub struct Page<V> {
    pub items: Vec<V>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

impl<V: Deref<Target = Value>> Page<V> {
    pub fn ids(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|item| id_of(item).to_owned())
            .collect()
    }

    pub fn item(&self, id: impl Display) -> &V {
        let id = id.to_string();
        self.items
            .iter()
            .find(|item| id_of(item) == id)
            .expect("the page holds the item")
    }
}

fn id_of(item: &Value) -> &str {
    item["id"].as_str().expect("an item has an id")
}

/// Declares a driver's newtype over a JSON response body: compared whole with
/// `==`, read through `Deref<Target = Value>`, and built by the driver from an
/// aggregate or published record with `Self(json!(..))`.
#[macro_export]
macro_rules! json_newtype {
    ($(#[$meta:meta])* $vis:vis $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, PartialEq)]
        $vis struct $name($crate::testing::__private::Value);

        impl $name {
            pub fn id(&self) -> &str {
                self.0["id"].as_str().expect(concat!(stringify!($name), " has an id"))
            }

            /// The version to send back in `If-Match`, for a versioned resource.
            pub fn version(&self) -> i64 {
                self.0["version"]
                    .as_str()
                    .and_then(|version| version.parse().ok())
                    .expect(concat!(stringify!($name), " has a version"))
            }
        }

        impl ::std::ops::Deref for $name {
            type Target = $crate::testing::__private::Value;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<'de> $crate::testing::__private::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
            where
                D: $crate::testing::__private::Deserializer<'de>,
            {
                <$crate::testing::__private::Value as $crate::testing::__private::Deserialize>::deserialize(
                    deserializer,
                )
                .map(Self)
            }
        }
    };
}

#[doc(hidden)]
pub mod __private {
    pub use serde::{Deserialize, Deserializer};
    pub use serde_json::{Value, json};
}
