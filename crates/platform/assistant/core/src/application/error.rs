use crate::domain::ConversationError;
use app_error::AppError;

impl From<ConversationError> for AppError {
    fn from(err: ConversationError) -> Self {
        // Exhaustive on purpose: a new domain error must choose its mapping.
        match err {
            ConversationError::EmptyMessage | ConversationError::NoToolCalls => {
                AppError::invalid_field("text", err.to_string())
            }
            ConversationError::AwaitingApproval | ConversationError::NotAwaitingApproval => {
                AppError::conflict(err.to_string())
            }
            ConversationError::NoSuchToolCall => AppError::not_found("tool call"),
        }
    }
}
