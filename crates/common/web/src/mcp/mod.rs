//! MCP over its streamable HTTP transport, for AI agents (ADR 0034). Every
//! operation of the API is a tool, described by its operation in the spec, and
//! each call is sent in-process through the endpoint the REST API is served
//! by. So a tool and its operation run the same code: auth, `If-Match`,
//! `Idempotency-Key`, validation and error bodies. `poem-mcpserver` speaks
//! the protocol; this binds each session to the token subject that opened it.

mod discovery;
mod schema;
mod sessions;
#[cfg(test)]
mod tests;
mod tools;

pub use discovery::{Discovery, METADATA_PATH, Metadata};
pub use tools::{OpenApiTools, RestOnly, Surface, problems};

use crate::{RequestIdMiddleware, TraceRequests};
use poem::{Endpoint, EndpointExt, IntoEndpoint, Request};
use poem_mcpserver::{McpServer, streamable_http};
use sessions::{Presented, SubjectSessions};
use std::sync::Arc;

/// Where the server answers MCP.
pub const PATH: &str = "/mcp";

/// The MCP endpoint, offering the tools of `S`, each call sent through `api`:
/// the endpoint serving `S::Api` at `S::BASE`, with its middleware and request
/// data, but not the whole app, so a call never re-enters MCP. Every request
/// needs a valid bearer token, of the subject who opened its session; one
/// without is challenged to sign in as `discovery` says.
pub fn endpoint<S: Surface>(api: impl Endpoint + 'static, discovery: Discovery) -> impl Endpoint {
    // Its own request id, so a tool's refusal names one even where the MCP
    // request's isn't in scope: an SSE answer runs after the request returns.
    // And its own span, a child of the tool call's: the app's `TraceRequests`
    // is outside `/mcp`, so it sees the MCP request, not the call it makes.
    let api = Arc::new(
        api.with(TraceRequests::of::<S::Api>(S::BASE))
            .with(RequestIdMiddleware)
            .boxed(),
    );
    streamable_http::endpoint(move |req: &Request| {
        // The factory is synchronous, so it can't check the token itself.
        let presented = req
            .data::<Presented>()
            .cloned()
            .expect("SubjectSessions names the token of every request it lets through");
        McpServer::new()
            .with_server_info(S::NAME, S::VERSION)
            .tools(OpenApiTools::<S>::new(api.clone(), presented))
    })
    .into_endpoint()
    .with(SubjectSessions::new(discovery))
}
