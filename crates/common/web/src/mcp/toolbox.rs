//! The tools of a [`Surface`] for code in the server itself, such as a module's
//! agent: the same tools `/mcp` lists, each call sent in-process through the
//! REST endpoint as `/mcp` sends it, presenting the token the caller gives.

use super::Surface;
use super::fields::{ToolArgument, ToolField};
use super::tools::{Described, Unsent, described, send};
use crate::metrics::ToolCalls;
use crate::{RequestIdMiddleware, TraceRequests};
use poem::endpoint::BoxEndpoint;
use poem::http::Method;
use poem::{Endpoint, EndpointExt, Request, Response, Result};
use serde_json::Value;
use std::sync::{Arc, OnceLock, Weak};

/// A tool as a model is offered it.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentTool {
    /// The operation's id.
    pub name: &'static str,
    /// The operation's summary and description.
    pub description: &'static str,
    /// The JSON Schema of the tool's arguments.
    pub input_schema: Value,
    /// Whether its operation is a `GET`, which changes nothing.
    pub reads: bool,
    /// Its operation's path template under the API's base, such as `/notes/{id}`.
    pub path: &'static str,
    pub arguments: Vec<ToolArgument>,
    /// The values its arguments carry, in the order its schemas declare them.
    pub fields: Vec<ToolField>,
}

/// What a tool's operation answered: its status, and its body as JSON (a
/// body that isn't JSON as a string, an empty one as `null`).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolAnswer {
    pub status: u16,
    pub body: Value,
}

impl ToolAnswer {
    pub fn succeeded(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Why a tool call sent no request: there is no such tool, or its arguments
/// make no request of its operation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotSent {
    #[error("no tool is named `{0}`")]
    NoSuchTool(String),
    #[error("{0}")]
    Arguments(String),
}

/// The tools of a surface, calling through an API it is [served](Self::serve)
/// by. Clones share it. In request data of that API, it holds the API only
/// weakly, so the two don't keep each other alive.
#[derive(Clone)]
pub struct Toolbox {
    tools: Arc<[Described]>,
    listed: Arc<[AgentTool]>,
    base: &'static str,
    name: &'static str,
    instructions: &'static str,
    api: Arc<OnceLock<BoxEndpoint<'static, Response>>>,
    traced: TraceRequests,
    calls: Arc<ToolCalls>,
}

impl Toolbox {
    /// The tools of `S`, calling nothing until [served](Self::serve).
    pub fn new<S: Surface>() -> Self {
        let tools: Arc<[Described]> = described::<S>().0.into();
        let listed = tools
            .iter()
            .map(|tool| AgentTool {
                name: tool.name,
                description: tool.description,
                input_schema: tool.input_schema.clone(),
                reads: tool.method == Method::GET,
                path: tool.path,
                arguments: tool.arguments(),
                fields: tool.fields.clone(),
            })
            .collect();
        Self {
            tools,
            listed,
            base: S::BASE,
            name: S::NAME,
            instructions: S::INSTRUCTIONS,
            api: Arc::default(),
            traced: TraceRequests::of::<S::Api>(S::BASE),
            calls: Arc::new(ToolCalls::new()),
        }
    }

    /// Calls go through `api`, the endpoint serving `S::Api` at `S::BASE`, as
    /// `/mcp`'s do: with a request id and a span of their own. Only the first
    /// call counts.
    pub fn serve(&self, api: &Arc<BoxEndpoint<'static, Response>>) {
        let api = Served(Arc::downgrade(api))
            .with(self.traced.clone())
            .with(RequestIdMiddleware)
            .boxed();
        let _ = self.api.set(api);
    }

    /// The server's name, as an MCP client is told it.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// What an agent should know to use the tools: the MCP server's instructions.
    pub fn instructions(&self) -> &'static str {
        self.instructions
    }

    pub fn tools(&self) -> &[AgentTool] {
        &self.listed
    }

    /// Calls the tool `name` with `arguments`, presenting `token`.
    pub async fn call(
        &self,
        name: &str,
        arguments: Value,
        token: &str,
    ) -> std::result::Result<ToolAnswer, NotSent> {
        let api = self
            .api
            .get()
            .expect("a toolbox is served by its API before a tool is called");
        let operation = self.tools.iter().find(|tool| tool.name == name);
        let answer = send(
            api,
            &self.calls,
            self.base,
            name,
            operation,
            arguments,
            token,
        )
        .await
        .map_err(|unsent| match unsent {
            Unsent::NoSuchTool => NotSent::NoSuchTool(name.to_owned()),
            Unsent::Arguments(err) => NotSent::Arguments(err),
        })?;
        let body = if answer.body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&answer.body)
                .unwrap_or_else(|_| String::from_utf8_lossy(&answer.body).into_owned().into())
        };
        Ok(ToolAnswer {
            status: answer.status.as_u16(),
            body,
        })
    }
}

/// The API, while it is still served.
struct Served(Weak<BoxEndpoint<'static, Response>>);

impl Endpoint for Served {
    type Output = Response;

    async fn call(&self, req: Request) -> Result<Response> {
        let api = self.0.upgrade().ok_or_else(|| {
            poem::Error::from_string(
                "the API is no longer served",
                poem::http::StatusCode::SERVICE_UNAVAILABLE,
            )
        })?;
        api.call(req).await
    }
}
