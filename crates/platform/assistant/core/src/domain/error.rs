#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConversationError {
    #[error("a message must have text")]
    EmptyMessage,
    #[error("a message of tool calls must hold at least one")]
    NoToolCalls,
}
