use app_web::{Gate, Performs};

/// The assistant module's gate, `module:assistant` / `use` (ADR 0040).
pub struct AssistantGate;

impl Gate for AssistantGate {
    const MODULE: &'static str = "assistant";
}

pub type Caller = app_web::Caller<AssistantGate>;

/// The action each operation performs, as an admin granting it sees it. Every one also needs
/// `chat` on the assistant, which `get_assistant_status` names.
pub const OPERATIONS: &[Performs] = &[
    Performs::new("get_assistant_status", "assistant", "chat"),
    Performs::new("start_conversation", "conversation", "start"),
    Performs::new("list_conversations", "conversation", "list"),
    Performs::new("get_conversation", "conversation", "view"),
    Performs::new("delete_conversation", "conversation", "delete"),
    Performs::new("send_message", "conversation", "post"),
];
