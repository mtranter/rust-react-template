//! Ids, timestamps, and the version are plain strings so the generated TS client stays
//! dependency-free.

use crate::application::{ConversationSortKey, ConversationSummary, Status, Versioned};
use crate::domain::{Content, Conversation, Message, Outcome, ToolCall, ToolCallStatus};
use app_persistence::{Direction, Sort};
use app_web::SortParam;
use poem_openapi::{Enum, Object, Union};

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct AssistantStatusResponse {
    /// Whether this app has a model to chat with. Off, there is no assistant to talk to.
    pub on: bool,
}

impl From<Status> for AssistantStatusResponse {
    fn from(status: Status) -> Self {
        Self { on: status.on }
    }
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct ConversationResponse {
    pub id: String,
    /// The start of the caller's first message; empty until there is one.
    pub title: String,
    pub created_at: String,
    /// Oldest first.
    pub messages: Vec<MessageResponse>,
    /// Optimistic-concurrency version. Echo it back in `If-Match` to change or delete this
    /// conversation; a stale value is refused with `412 Precondition Failed`. Also mirrored in
    /// the `ETag` response header.
    pub version: String,
}

impl From<Versioned<Conversation>> for ConversationResponse {
    fn from(versioned: Versioned<Conversation>) -> Self {
        let Versioned { entity: c, version } = versioned;
        Self {
            id: c.id().to_string(),
            title: c.title().to_string(),
            created_at: c.created_at().to_rfc3339(),
            messages: c.messages().iter().map(MessageResponse::from).collect(),
            version: version.as_i64().to_string(),
        }
    }
}

/// A conversation as a list shows it, without its messages.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct ConversationSummaryResponse {
    pub id: String,
    /// The start of the caller's first message; empty until there is one.
    pub title: String,
    pub created_at: String,
    /// Its version, as `getConversation` answers it.
    pub version: String,
}

impl From<Versioned<ConversationSummary>> for ConversationSummaryResponse {
    fn from(versioned: Versioned<ConversationSummary>) -> Self {
        let Versioned { entity: c, version } = versioned;
        Self {
            id: c.id.to_string(),
            title: c.title,
            created_at: c.created_at.to_rfc3339(),
            version: version.as_i64().to_string(),
        }
    }
}

/// One of a conversation's messages, told apart by its `kind`.
#[derive(Debug, Union)]
#[oai(discriminator_name = "kind", one_of)]
pub enum MessageResponse {
    #[oai(mapping = "user")]
    User(UserMessageResponse),
    #[oai(mapping = "assistant")]
    Assistant(AssistantMessageResponse),
    #[oai(mapping = "toolCalls")]
    ToolCalls(ToolCallsMessageResponse),
}

/// What the caller wrote.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct UserMessageResponse {
    pub id: String,
    pub posted_at: String,
    pub text: String,
}

/// What the assistant answered in words.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct AssistantMessageResponse {
    pub id: String,
    pub posted_at: String,
    pub text: String,
}

/// The app's operations the assistant asked to run as the caller.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct ToolCallsMessageResponse {
    pub id: String,
    pub posted_at: String,
    /// In the order the assistant asked for them.
    pub tool_calls: Vec<ToolCallResponse>,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct ToolCallResponse {
    pub id: String,
    /// The operation's id, which is its tool's name.
    pub tool: String,
    pub arguments: serde_json::Value,
    pub status: ToolCallStatusResponse,
    /// The operation's answer when it succeeded.
    #[oai(skip_serializing_if_is_none)]
    pub result: Option<serde_json::Value>,
    /// The operation's error body when it was refused or failed, or why it didn't run.
    #[oai(skip_serializing_if_is_none)]
    pub error: Option<serde_json::Value>,
}

/// `ran`: a read, run at once; `awaiting_approval`: a write waiting for the caller to approve
/// or reject it; `approved`, `rejected`: the caller's decision.
#[derive(Debug, Clone, Copy, Enum)]
#[oai(rename_all = "snake_case")]
pub enum ToolCallStatusResponse {
    Ran,
    AwaitingApproval,
    Approved,
    Rejected,
}

impl From<&Message> for MessageResponse {
    fn from(message: &Message) -> Self {
        let id = message.id().to_string();
        let posted_at = message.posted_at().to_rfc3339();
        match message.content() {
            Content::User(text) => Self::User(UserMessageResponse {
                id,
                posted_at,
                text: text.clone(),
            }),
            Content::Assistant(text) => Self::Assistant(AssistantMessageResponse {
                id,
                posted_at,
                text: text.clone(),
            }),
            Content::ToolCalls(calls) => Self::ToolCalls(ToolCallsMessageResponse {
                id,
                posted_at,
                tool_calls: calls.iter().map(ToolCallResponse::from).collect(),
            }),
        }
    }
}

impl From<&ToolCall> for ToolCallResponse {
    fn from(call: &ToolCall) -> Self {
        let (result, error) = match call.outcome() {
            None => (None, None),
            Some(Outcome::Ok(body)) => (Some(body.clone()), None),
            Some(Outcome::Error(body)) => (None, Some(body.clone())),
        };
        Self {
            id: call.id().to_string(),
            tool: call.tool().to_string(),
            arguments: call.arguments().clone(),
            status: match call.status() {
                ToolCallStatus::Ran => ToolCallStatusResponse::Ran,
                ToolCallStatus::AwaitingApproval => ToolCallStatusResponse::AwaitingApproval,
                ToolCallStatus::Approved => ToolCallStatusResponse::Approved,
                ToolCallStatus::Rejected => ToolCallStatusResponse::Rejected,
            },
            result,
            error,
        }
    }
}

#[derive(Debug, Clone, Copy, Enum)]
pub enum ConversationSort {
    #[oai(rename = "createdAt")]
    CreatedAtAsc,
    #[oai(rename = "-createdAt")]
    CreatedAtDesc,
}

impl SortParam for ConversationSort {
    type Key = ConversationSortKey;

    fn sort(self) -> Sort<ConversationSortKey> {
        let direction = match self {
            Self::CreatedAtAsc => Direction::Asc,
            Self::CreatedAtDesc => Direction::Desc,
        };
        Sort {
            key: ConversationSortKey::CreatedAt,
            direction,
        }
    }
}
