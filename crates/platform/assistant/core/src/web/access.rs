use app_web::Performs;

/// The action each operation performs, as an admin granting it sees it. Every one also needs
/// `chat` on the assistant, which `get_assistant_status` names.
pub const OPERATIONS: &[Performs] = &[
    Performs::new("get_assistant_status", "assistant", "chat"),
    Performs::new("start_conversation", "conversation", "start"),
    Performs::new("list_conversations", "conversation", "list"),
    Performs::new("get_conversation", "conversation", "view"),
    Performs::new("delete_conversation", "conversation", "delete"),
    Performs::new("send_message", "conversation", "post"),
    Performs::new("decide_tool_call", "conversation", "post"),
];
