use super::Conversation;
use app_authz::{Resource, Value};

/// The assistant itself, as its policies and grants name it (ADR 0040): what a caller may do
/// with it before any conversation exists. Every conversation's use case asks for `chat` first,
/// so revoking it closes the assistant to its holder.
pub struct Assistant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum AssistantAction {
    Chat,
}

impl Resource for Assistant {
    const KIND: &'static str = "assistant";
    const ATTRS: &'static [&'static str] = &[];
    type Action = AssistantAction;

    fn attr(&self, _name: &str) -> Option<Value> {
        None
    }
}

/// What can be done to a conversation, as its policies and grants name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum ConversationAction {
    Start,
    List,
    View,
    Delete,
}

impl Resource for Conversation {
    const KIND: &'static str = "conversation";
    const ATTRS: &'static [&'static str] = &["ownerId"];
    type Action = ConversationAction;
    const VIEW: Option<ConversationAction> = Some(ConversationAction::View);

    fn attr(&self, name: &str) -> Option<Value> {
        match name {
            "ownerId" => Some(self.owner_id().to_string().into()),
            _ => None,
        }
    }
}
