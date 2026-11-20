//! The one seam between the assistant and a model provider (Spec 0012): Bedrock in
//! `crate::bedrock`, a scripted fake in tests, another provider a new implementation. Its types
//! are the module's own, so a bump of rig, which is pre-1.0, stops at the adapter.

use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait ChatModel: Send + Sync {
    /// The model's next step: words, tool calls, or both. Each piece of its words goes to
    /// `text` as it is generated, before the whole reply is answered.
    async fn reply(
        &self,
        request: ModelRequest,
        text: &(dyn for<'a> Fn(&'a str) + Send + Sync),
    ) -> Result<ModelReply, ModelError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRequest {
    pub system: String,
    /// Oldest first, starting with the owner's words. A run of messages from one side is one
    /// message of the provider's: an assistant's words and the tool calls after them, or a
    /// step's tool results and the owner's next words.
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ModelTool>,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModelMessage {
    User(String),
    Assistant(String),
    ToolCalls(Vec<ModelToolCall>),
    ToolResults(Vec<ModelToolResult>),
}

/// A tool the model may call.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelTool {
    pub name: String,
    pub description: String,
    /// The JSON Schema of its arguments.
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelToolCall {
    /// Ours, not the provider's: the stored tool call's, so a later turn sends the same.
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelToolResult {
    /// The call it answers.
    pub id: String,
    pub name: String,
    /// The operation's answer, as text.
    pub content: String,
    /// Whether it was refused or failed, or didn't run.
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModelReply {
    pub text: String,
    /// In the order the model asked for them. Their ids are the provider's, if any.
    pub tool_calls: Vec<ModelToolCall>,
    pub output_tokens: u32,
    /// Whether the reply was cut short at `max_output_tokens`.
    pub truncated: bool,
}

/// The provider refused or failed: throttled, denied, or no such model.
#[derive(Debug, thiserror::Error)]
#[error("the model failed: {0}")]
pub struct ModelError(pub String);
