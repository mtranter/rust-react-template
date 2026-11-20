//! The platform's `assistant` module (Spec 0012): a chat in which a model acts in the app as the
//! signed-in user, on with a model configured (`APP_AGENT_MODEL`) and off without one, and the
//! Conversations each user holds with it. Platform, not the app's: it stays registered after
//! `make init`.

mod application;
mod bedrock;
mod db;
mod di;
mod domain;
#[cfg(feature = "seed")]
pub mod seed;
mod web;

pub use application::{
    ChatModel, ModelError, ModelMessage, ModelReply, ModelRequest, ModelTool, ModelToolCall,
    ModelToolResult, Settings,
};
pub use db::{ConversationColumns, MIGRATOR, SCHEMA, migrations};
pub use di::{AssistantProvider, module, module_with, policies, provider};
pub use domain::{
    Assistant, AssistantAction, Content, Conversation, ConversationAction, ConversationError,
    ConversationId, Message, MessageId, Outcome, ToolCall, ToolCallId, ToolCallStatus,
};
pub use web::AssistantEndpoints;
