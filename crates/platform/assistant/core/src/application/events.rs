use super::{PendingEvent, Version};
use crate::domain::{Conversation, ConversationEvent};
use assistant_api::events::v1::{self, ConversationTopic};

/// Where the domain's events meet the published contract.
pub(crate) fn pending(
    conversation: &Conversation,
    version: Version,
    event: ConversationEvent,
) -> PendingEvent {
    let event = match event {
        ConversationEvent::Started => v1::ConversationEvent::Started,
        ConversationEvent::MessagePosted(message) => {
            v1::ConversationEvent::MessagePosted(v1::OfMessage {
                message_id: message.as_uuid(),
            })
        }
        ConversationEvent::Deleted => v1::ConversationEvent::Deleted,
    };
    PendingEvent::on::<ConversationTopic>(version, &state(conversation), event)
}

fn state(conversation: &Conversation) -> v1::Conversation {
    v1::Conversation {
        id: conversation.id().as_uuid(),
        owner_id: conversation.owner_id().to_string(),
        title: conversation.title().to_string(),
        created_at: conversation.created_at(),
    }
}
