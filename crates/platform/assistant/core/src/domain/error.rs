#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConversationError {
    #[error("a message must have text")]
    EmptyMessage,
    #[error("a message of tool calls must hold at least one")]
    NoToolCalls,
    #[error("the assistant is waiting for a decision on an action it asked to take")]
    AwaitingApproval,
}
