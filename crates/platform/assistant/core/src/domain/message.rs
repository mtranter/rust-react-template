use super::{ConversationError, MessageId, ToolCallId};
use chrono::{DateTime, Utc};
use serde_json::Value;

/// One turn's worth of a [`Conversation`](super::Conversation): a child of it, loaded, changed,
/// saved and announced only through it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    id: MessageId,
    posted_at: DateTime<Utc>,
    content: Content,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// What the owner wrote.
    User(String),
    /// What the model answered in words.
    Assistant(String),
    /// What the model asked to run, in the order it asked.
    ToolCalls(Vec<ToolCall>),
}

/// An operation of the app the model asked to run as the owner, and how that went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    id: ToolCallId,
    /// The operation's id, which is its tool's name.
    tool: String,
    arguments: Value,
    status: ToolCallStatus,
    outcome: Option<Outcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum ToolCallStatus {
    /// A read, run as soon as the model asked.
    Ran,
    /// A write, waiting for the owner to approve or reject it.
    AwaitingApproval,
    Approved,
    Rejected,
}

/// The operation's answer: its body on success, its `ErrorBody` on a refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ok(Value),
    Error(Value),
}

impl Message {
    pub fn user(id: MessageId, text: &str, now: DateTime<Utc>) -> Result<Self, ConversationError> {
        Ok(Self::new(id, Content::User(text_of(text)?), now))
    }

    pub fn assistant(
        id: MessageId,
        text: &str,
        now: DateTime<Utc>,
    ) -> Result<Self, ConversationError> {
        Ok(Self::new(id, Content::Assistant(text_of(text)?), now))
    }

    pub fn tool_calls(
        id: MessageId,
        calls: Vec<ToolCall>,
        now: DateTime<Utc>,
    ) -> Result<Self, ConversationError> {
        if calls.is_empty() {
            return Err(ConversationError::NoToolCalls);
        }
        Ok(Self::new(id, Content::ToolCalls(calls), now))
    }

    fn new(id: MessageId, content: Content, posted_at: DateTime<Utc>) -> Self {
        Self {
            id,
            posted_at,
            content,
        }
    }

    /// Skips invariants: for repositories rebuilding persisted state only.
    pub fn restore(id: MessageId, posted_at: DateTime<Utc>, content: Content) -> Self {
        Self::new(id, content, posted_at)
    }

    pub fn id(&self) -> MessageId {
        self.id
    }

    pub fn posted_at(&self) -> DateTime<Utc> {
        self.posted_at
    }

    pub fn content(&self) -> &Content {
        &self.content
    }
}

impl ToolCall {
    pub fn new(
        id: ToolCallId,
        tool: String,
        arguments: Value,
        status: ToolCallStatus,
        outcome: Option<Outcome>,
    ) -> Self {
        Self {
            id,
            tool,
            arguments,
            status,
            outcome,
        }
    }

    pub fn id(&self) -> ToolCallId {
        self.id
    }

    pub fn tool(&self) -> &str {
        &self.tool
    }

    pub fn arguments(&self) -> &Value {
        &self.arguments
    }

    pub fn status(&self) -> ToolCallStatus {
        self.status
    }

    /// None until it has run, or been rejected.
    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }
}

fn text_of(raw: &str) -> Result<String, ConversationError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(ConversationError::EmptyMessage);
    }
    Ok(text.to_string())
}
