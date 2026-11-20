//! What `assistant` announces (ADR 0012): one topic per aggregate root, each event carrying the
//! root as it now is.

pub mod v1 {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    /// What every conversation event carries: the conversation but its messages, which are its
    /// owner's words and the tools' answers to them, and stay in the assistant. Only gains
    /// fields, each `#[serde(default)]`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Conversation {
        pub id: Uuid,
        /// The subject of the token that started it.
        pub owner_id: String,
        /// Empty until its first message.
        pub title: String,
        pub created_at: DateTime<Utc>,
    }

    /// Which of the conversation's messages an event is about.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct OfMessage {
        pub message_id: Uuid,
    }

    app_events::topic! {
        /// Everything that happens to a conversation, keyed by its id.
        pub topic ConversationTopic("assistant.conversation.v1") {
            source: "/assistant",
            retention: app_events::TimeDelta::days(7),
            entity: Conversation,
            key: |conversation| conversation.id.to_string(),
            events: ConversationEvent, ConversationEventKind;
            ConversationStarted => Started: "assistant.conversation_started",
            ConversationMessagePosted => MessagePosted(OfMessage): "assistant.conversation_message_posted",
            ConversationDeleted => Deleted: "assistant.conversation_deleted",
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn every_event_round_trips() {
            let events = [
                ConversationEvent::Started,
                ConversationEvent::MessagePosted(OfMessage {
                    message_id: Uuid::nil(),
                }),
                ConversationEvent::Deleted,
            ];
            for event in &events {
                let (event_type, context) = event.encode().unwrap();
                assert_eq!(
                    &ConversationEvent::decode(event_type, context).unwrap(),
                    event
                );
            }
            assert_eq!(events.len(), ConversationEventKind::ALL.len());
        }
    }
}
