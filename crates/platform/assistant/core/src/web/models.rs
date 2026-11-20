//! Ids, timestamps, and the version are plain strings so the generated TS client stays
//! dependency-free.

use crate::application::{
    ConversationSortKey, ConversationSummary, Status, TurnEvent, Verdict, Versioned,
};
use crate::domain::{Content, Conversation, Message, Outcome, ToolCall, ToolCallStatus};
use app_persistence::{Direction, Sort};
use app_web::{ApiError, ErrorBody, SortParam};
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

impl ConversationResponse {
    /// With each tool call's summary, as `summary_of` tells it by the tool's name.
    pub fn summarised(mut self, summary_of: impl Fn(&str) -> Option<String>) -> Self {
        for message in &mut self.messages {
            if let MessageResponse::ToolCalls(message) = message {
                for call in &mut message.tool_calls {
                    call.summary = summary_of(&call.tool);
                }
            }
        }
        self
    }
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
    #[oai(mapping = "error")]
    Error(ErrorMessageResponse),
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

/// Why a turn ended in failure: the model failed, or the caller's sign-in expired. Shown to the
/// caller, never sent to the model.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct ErrorMessageResponse {
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
    /// The operation's summary, saying what it does; absent for a tool the app no longer has.
    #[oai(skip_serializing_if_is_none)]
    pub summary: Option<String>,
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
            Content::Error(text) => Self::Error(ErrorMessageResponse {
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

/// An outcome as `result` and `error`, one of which it has.
fn result_and_error(
    outcome: Option<&Outcome>,
) -> (Option<serde_json::Value>, Option<serde_json::Value>) {
    match outcome {
        None => (None, None),
        Some(Outcome::Ok(body)) => (Some(body.clone()), None),
        Some(Outcome::Error(body)) => (None, Some(body.clone())),
    }
}

impl From<ToolCallStatus> for ToolCallStatusResponse {
    fn from(status: ToolCallStatus) -> Self {
        match status {
            ToolCallStatus::Ran => Self::Ran,
            ToolCallStatus::AwaitingApproval => Self::AwaitingApproval,
            ToolCallStatus::Approved => Self::Approved,
            ToolCallStatus::Rejected => Self::Rejected,
        }
    }
}

impl From<&ToolCall> for ToolCallResponse {
    fn from(call: &ToolCall) -> Self {
        let (result, error) = result_and_error(call.outcome());
        Self {
            id: call.id().to_string(),
            tool: call.tool().to_string(),
            summary: None,
            arguments: call.arguments().clone(),
            status: call.status().into(),
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

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct SendMessageRequest {
    /// What the caller says to the assistant.
    pub text: String,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct DecideToolCallRequest {
    pub decision: DecisionRequest,
}

/// `approve` runs the operation as the caller; `reject` doesn't, and tells the assistant the
/// caller declined it.
#[derive(Debug, Clone, Copy, Enum)]
#[oai(rename_all = "snake_case")]
pub enum DecisionRequest {
    Approve,
    Reject,
}

impl From<DecisionRequest> for Verdict {
    fn from(decision: DecisionRequest) -> Self {
        match decision {
            DecisionRequest::Approve => Verdict::Approve,
            DecisionRequest::Reject => Verdict::Reject,
        }
    }
}

/// One event of a turn's stream, told apart by its `kind`. A turn ends with `finished`, or with
/// `error` when what it had to store couldn't be.
#[derive(Debug, Union)]
#[oai(discriminator_name = "kind", one_of)]
pub enum TurnEventResponse {
    #[oai(mapping = "text")]
    Text(TextEvent),
    #[oai(mapping = "toolCallStarted")]
    ToolCallStarted(ToolCallStartedEvent),
    #[oai(mapping = "toolResult")]
    ToolResult(ToolResultEvent),
    #[oai(mapping = "approvalRequired")]
    ApprovalRequired(ApprovalRequiredEvent),
    #[oai(mapping = "decided")]
    Decided(ToolCallDecidedEvent),
    #[oai(mapping = "finished")]
    Finished(TurnFinishedEvent),
    #[oai(mapping = "error")]
    Error(TurnErrorEvent),
}

/// A piece of the assistant's words, as they are generated: append it to those before.
#[derive(Debug, Object)]
pub struct TextEvent {
    pub text: String,
}

/// The assistant asked to run one of the app's operations as the caller.
#[derive(Debug, Object)]
pub struct ToolCallStartedEvent {
    pub id: String,
    /// The operation's id, which is its tool's name.
    pub tool: String,
    pub arguments: serde_json::Value,
}

/// A read's answer: `result` when it succeeded, `error` (its error body) when refused.
#[derive(Debug, Object)]
pub struct ToolResultEvent {
    /// The tool call's, as `toolCallStarted` named it.
    pub id: String,
    #[oai(skip_serializing_if_is_none)]
    pub result: Option<serde_json::Value>,
    #[oai(skip_serializing_if_is_none)]
    pub error: Option<serde_json::Value>,
}

/// A write the assistant asked for, stored `awaiting_approval` and not run: the caller decides
/// it, and the turn ends here.
#[derive(Debug, Object)]
pub struct ApprovalRequiredEvent {
    pub id: String,
    pub tool: String,
    /// The operation's summary, saying what approving it would do.
    pub summary: String,
    pub arguments: serde_json::Value,
}

/// The caller's decision on a call that awaited it, `approved` or `rejected`: a call approved
/// has run, and its answer follows as a `toolResult`.
#[derive(Debug, Object)]
pub struct ToolCallDecidedEvent {
    /// The tool call's, as `approvalRequired` named it.
    pub id: String,
    pub status: ToolCallStatusResponse,
}

/// The turn ended: the messages it stored, the caller's first, and the conversation's version
/// now, for the next `If-Match`.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct TurnFinishedEvent {
    pub message_ids: Vec<String>,
    pub version: String,
}

/// What went wrong mid-turn, as an error body: the model failed (`unavailable`), the caller's
/// sign-in expired (`unauthorized`), or the turn couldn't be stored.
#[derive(Debug, Object)]
pub struct TurnErrorEvent {
    pub error: ErrorBody,
}

impl From<TurnEvent> for TurnEventResponse {
    fn from(event: TurnEvent) -> Self {
        match event {
            TurnEvent::Text(text) => Self::Text(TextEvent { text }),
            TurnEvent::ToolCallStarted {
                id,
                tool,
                arguments,
            } => Self::ToolCallStarted(ToolCallStartedEvent {
                id: id.to_string(),
                tool,
                arguments,
            }),
            TurnEvent::ToolResult { id, outcome } => {
                let (result, error) = result_and_error(Some(&outcome));
                Self::ToolResult(ToolResultEvent {
                    id: id.to_string(),
                    result,
                    error,
                })
            }
            TurnEvent::ApprovalRequired {
                id,
                tool,
                summary,
                arguments,
            } => Self::ApprovalRequired(ApprovalRequiredEvent {
                id: id.to_string(),
                tool,
                summary,
                arguments,
            }),
            TurnEvent::Decided { id, status } => Self::Decided(ToolCallDecidedEvent {
                id: id.to_string(),
                status: status.into(),
            }),
            TurnEvent::Finished { messages, version } => Self::Finished(TurnFinishedEvent {
                message_ids: messages.iter().map(ToString::to_string).collect(),
                version: version.as_i64().to_string(),
            }),
            TurnEvent::Failed(err) => Self::Error(TurnErrorEvent {
                error: ApiError::from(err).into_body(),
            }),
        }
    }
}
