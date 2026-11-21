//! The app's operations as the assistant's tools (Spec 0012): the same surface `/mcp` serves,
//! called in-process as the user whose request runs the turn. A port, so this layer names no
//! web type; its adapter is in `crate::web`.

use async_trait::async_trait;
use serde_json::Value;

/// Bound to one request's token: every call presents it, so a tool can do exactly what that
/// user can.
#[async_trait]
pub trait AppTools: Send + Sync {
    /// The app's name, as its API is titled.
    fn app(&self) -> &str;
    /// What an agent should know to use the tools: the server's MCP instructions.
    fn instructions(&self) -> &str;
    fn tools(&self) -> Vec<AppTool>;
    /// A tool that doesn't exist, or arguments that make no request of it, is refused like
    /// any other call. `idempotency_key`, for a tool that takes one, replaces any the model
    /// gave.
    async fn call(&self, name: &str, arguments: Value, idempotency_key: Option<&str>) -> Answered;
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppTool {
    /// Its operation's id.
    pub name: String,
    pub description: String,
    /// The JSON Schema of its arguments.
    pub input_schema: Value,
    /// Whether its operation only reads (a `GET`), so it runs without the owner's approval.
    pub reads: bool,
    /// Its operation's path template, such as `/orders/{id}`.
    pub path: String,
    pub arguments: Vec<ToolArgument>,
    /// The values its arguments carry, in the order its schemas declare them.
    pub fields: Vec<ToolField>,
}

/// Where a tool's argument goes in its operation's request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentIn {
    Path,
    Query,
    Header,
    Body,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolArgument {
    pub name: String,
    pub sent: ArgumentIn,
}

/// One value a tool's argument carries: the argument, or a property within it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolField {
    pub argument: String,
    /// The properties, outermost first, it is under within the argument.
    pub properties: Vec<FieldProperty>,
    /// Its schema's `format`, or, for a list, its items'.
    pub format: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProperty {
    pub name: String,
    pub title: Option<String>,
}

impl AppTool {
    /// Its operation's summary: the first paragraph of its description.
    pub fn summary(&self) -> &str {
        summary(&self.description)
    }
}

/// The first paragraph of a tool's `description`, which is its operation's summary.
pub fn summary(description: &str) -> &str {
    description.split("\n\n").next().unwrap_or_default().trim()
}

/// The operation's answer: its status, and its body or `ErrorBody`.
#[derive(Debug, Clone, PartialEq)]
pub struct Answered {
    pub status: u16,
    pub body: Value,
}

impl Answered {
    pub fn succeeded(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The token it presented has expired, or was never valid.
    pub fn unauthorized(&self) -> bool {
        self.status == 401
    }
}
