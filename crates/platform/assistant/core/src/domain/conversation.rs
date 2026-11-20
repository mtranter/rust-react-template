use super::{
    Content, ConversationError, ConversationEvent, ConversationId, Decision, Message, ToolCall,
    ToolCallId, ToolCallStatus,
};
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

    /// The owner says `message`, opening a turn: refused while the assistant waits for their
    /// decision on an action, which a turn can't go on without.
    pub fn ask(&mut self, message: Message) -> Result<ConversationEvent, ConversationError> {
        if self.awaiting_approval().next().is_some() {
            return Err(ConversationError::AwaitingApproval);
        }
        Ok(self.post(message))
    }

    /// Every tool call still waiting for the owner's decision, oldest first.
    pub fn awaiting_approval(&self) -> impl Iterator<Item = &ToolCall> {
        self.messages.iter().flat_map(Message::awaiting_approval)
    }

    /// Call `id`, if it is still waiting for the owner's decision.
    pub fn awaiting(&self, id: ToolCallId) -> Result<&ToolCall, ConversationError> {
        let call = self
            .messages
            .iter()
            .flat_map(Message::calls)
            .find(|call| call.id() == id)
            .ok_or(ConversationError::NoSuchToolCall)?;
        match call.status() {
            ToolCallStatus::AwaitingApproval => Ok(call),
            _ => Err(ConversationError::NotAwaitingApproval),
        }
    }

    /// Records the owner's `decision` on call `id`: refused once it has had one.
    pub fn decide(
        &mut self,
        id: ToolCallId,
        decision: Decision,
    ) -> Result<ConversationEvent, ConversationError> {
        self.messages
            .iter_mut()
            .flat_map(Message::calls_mut)
            .find(|call| call.id() == id)
            .ok_or(ConversationError::NoSuchToolCall)?
            .decide(decision)?;
        Ok(ConversationEvent::ToolCallDecided(id))
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
    use crate::domain::{MessageId, Outcome};
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

    fn awaiting(conversation: &mut Conversation) -> ToolCallId {
        let id = ToolCallId::new();
        let call = ToolCall::new(
            id,
            "create_list".into(),
            json!({}),
            ToolCallStatus::AwaitingApproval,
            None,
        );
        let _ = conversation
            .post(Message::tool_calls(MessageId::new(), vec![call], Utc::now()).unwrap());
        id
    }

    #[test]
    fn an_approved_call_keeps_its_answer_and_a_rejected_one_none() {
        let mut conversation = started();
        let (approved, rejected) = (awaiting(&mut conversation), awaiting(&mut conversation));
        let answer = Outcome::Ok(json!({ "id": 1 }));
        assert_eq!(
            conversation.decide(approved, Decision::Approved(answer.clone())),
            Ok(ConversationEvent::ToolCallDecided(approved))
        );
        assert_eq!(
            conversation.decide(rejected, Decision::Rejected),
            Ok(ConversationEvent::ToolCallDecided(rejected))
        );
        let calls: Vec<_> = conversation
            .messages()
            .iter()
            .flat_map(Message::calls)
            .collect();
        assert_eq!(calls[0].status(), ToolCallStatus::Approved);
        assert_eq!(calls[0].outcome(), Some(&answer));
        assert_eq!(calls[1].status(), ToolCallStatus::Rejected);
        assert_eq!(calls[1].outcome(), None);
        assert!(conversation.awaiting_approval().next().is_none());
        assert!(conversation.ask(said("Thanks")).is_ok());
    }

    #[test]
    fn a_call_is_decided_once() {
        let mut conversation = started();
        let id = awaiting(&mut conversation);
        assert!(conversation.awaiting(id).is_ok());
        let _ = conversation.decide(id, Decision::Rejected).unwrap();
        assert_eq!(
            conversation.awaiting(id).map(ToolCall::id),
            Err(ConversationError::NotAwaitingApproval)
        );
        assert_eq!(
            conversation.decide(id, Decision::Rejected),
            Err(ConversationError::NotAwaitingApproval)
        );
        assert_eq!(
            conversation.decide(ToolCallId::new(), Decision::Rejected),
            Err(ConversationError::NoSuchToolCall)
        );
    }

    #[test]
    fn the_owner_cant_ask_while_an_action_awaits_their_decision() {
        let mut conversation = started();
        let call = ToolCall::new(
            ToolCallId::new(),
            "create_note".into(),
            json!({}),
            ToolCallStatus::AwaitingApproval,
            None,
        );
        let _ = conversation
            .post(Message::tool_calls(MessageId::new(), vec![call], Utc::now()).unwrap());
        assert_eq!(
            conversation.ask(said("Never mind")),
            Err(ConversationError::AwaitingApproval)
        );
        assert_eq!(conversation.messages().len(), 1);
    }
}
