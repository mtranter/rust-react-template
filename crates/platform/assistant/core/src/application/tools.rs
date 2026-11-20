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
    /// any other call.
    async fn call(&self, name: &str, arguments: Value) -> Answered;
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
