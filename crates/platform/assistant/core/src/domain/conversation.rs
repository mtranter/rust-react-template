use super::{Content, ConversationEvent, ConversationId, Message};
use chrono::{DateTime, Utc};

/// A title is the start of the first thing its owner said.
const MAX_TITLE_CHARS: usize = 80;

/// A chat between its owner and the assistant (Spec 0012): the aggregate root of its messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    id: ConversationId,
    /// The subject of the token that started it.
    owner_id: String,
    title: String,
    created_at: DateTime<Utc>,
    messages: Vec<Message>,
}

impl Conversation {
    /// Empty and untitled, owned by `owner_id`.
    pub fn start(
        id: ConversationId,
        owner_id: String,
        now: DateTime<Utc>,
    ) -> (Self, ConversationEvent) {
        let conversation = Self {
            id,
            owner_id,
            title: String::new(),
            created_at: now,
            messages: Vec::new(),
        };
        (conversation, ConversationEvent::Started)
    }

    /// Adds `message` as the latest. The first the owner posts titles the conversation.
    pub fn post(&mut self, message: Message) -> ConversationEvent {
        if let (true, Content::User(text)) = (self.title.is_empty(), message.content()) {
            self.title = title(text);
        }
        let id = message.id();
        self.messages.push(message);
        ConversationEvent::MessagePosted(id)
    }

    /// What deleting it says happened.
    pub fn delete(&self) -> ConversationEvent {
        ConversationEvent::Deleted
    }

    /// Skips invariants: for repositories rebuilding persisted state only.
    pub fn restore(
        id: ConversationId,
        owner_id: String,
        title: String,
        created_at: DateTime<Utc>,
        messages: Vec<Message>,
    ) -> Self {
        Self {
            id,
            owner_id,
            title,
            created_at,
            messages,
        }
    }

    pub fn id(&self) -> ConversationId {
        self.id
    }

    pub fn owner_id(&self) -> &str {
        &self.owner_id
    }

    /// Empty until the owner's first message.
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Oldest first.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }
}

/// The first line of `text`, cut at a character boundary.
fn title(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    match line.char_indices().nth(MAX_TITLE_CHARS) {
        Some((cut, _)) => format!("{}…", line[..cut].trim_end()),
        None => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ConversationError, MessageId, ToolCall, ToolCallId, ToolCallStatus};
    use serde_json::json;

    fn started() -> Conversation {
        Conversation::start(ConversationId::new(), "alice".into(), Utc::now()).0
    }

    fn said(text: &str) -> Message {
        Message::user(MessageId::new(), text, Utc::now()).unwrap()
    }

    #[test]
    fn a_conversation_starts_untitled_and_empty() {
        let (conversation, event) =
            Conversation::start(ConversationId::new(), "alice".into(), Utc::now());
        assert_eq!(event, ConversationEvent::Started);
        assert_eq!(conversation.title(), "");
        assert!(conversation.messages().is_empty());
    }

    #[test]
    fn the_owners_first_message_titles_it_and_later_ones_dont() {
        let mut conversation = started();
        let reply = Message::assistant(MessageId::new(), "Hello", Utc::now()).unwrap();
        let _ = conversation.post(reply);
        assert_eq!(conversation.title(), "");

        let first = said("  What roles are there?\nAnd who holds them?");
        let id = first.id();
        assert_eq!(
            conversation.post(first),
            ConversationEvent::MessagePosted(id)
        );
        let _ = conversation.post(said("Something else"));
        assert_eq!(conversation.title(), "What roles are there?");
        assert_eq!(conversation.messages().len(), 3);
    }

    #[test]
    fn a_long_first_message_is_cut_short_for_its_title() {
        let mut conversation = started();
        let _ = conversation.post(said(&"é".repeat(MAX_TITLE_CHARS + 5)));
        assert_eq!(
            conversation.title(),
            format!("{}…", "é".repeat(MAX_TITLE_CHARS))
        );
    }

    #[test]
    fn a_message_needs_text_and_tool_calls_need_a_call() {
        assert_eq!(
            Message::user(MessageId::new(), "  ", Utc::now()),
            Err(ConversationError::EmptyMessage)
        );
        assert_eq!(
            Message::tool_calls(MessageId::new(), vec![], Utc::now()),
            Err(ConversationError::NoToolCalls)
        );
        let call = ToolCall::new(
            ToolCallId::new(),
            "list_roles".into(),
            json!({}),
            ToolCallStatus::AwaitingApproval,
            None,
        );
        assert!(Message::tool_calls(MessageId::new(), vec![call], Utc::now()).is_ok());
    }
}
