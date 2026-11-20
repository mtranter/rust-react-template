//! The one seam between the assistant and a model provider (Spec 0012): Bedrock in
//! `crate::bedrock`, a scripted fake in tests, another provider a new implementation. Its types
//! are the module's own, so a bump of rig, which is pre-1.0, stops at the adapter.

use async_trait::async_trait;

#[async_trait]
pub trait ChatModel: Send + Sync {
    async fn reply(&self, request: ModelRequest) -> Result<ModelReply, ModelError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequest {
    pub system: String,
    /// Oldest first; the last is the user's latest.
    pub messages: Vec<ModelMessage>,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelMessage {
    User(String),
    Assistant(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelReply {
    pub text: String,
}

/// The provider refused or failed: throttled, denied, or no such model.
#[derive(Debug, thiserror::Error)]
#[error("the model failed: {0}")]
pub struct ModelError(pub String);
