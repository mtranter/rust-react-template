use super::{MessageId, ToolCallId};

/// What happened to a [`Conversation`](super::Conversation), as its transitions decide; the
/// service announces each one as the matching `assistant_api` event.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationEvent {
    Started,
    MessagePosted(MessageId),
    /// The owner approved or rejected a tool call awaiting their decision.
    ToolCallDecided(ToolCallId),
    Deleted,
}
