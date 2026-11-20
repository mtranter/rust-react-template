use app_web::{Gate, Performs};

/// The assistant module's gate, `module:assistant` / `use` (ADR 0040).
pub struct AssistantGate;

impl Gate for AssistantGate {
    const MODULE: &'static str = "assistant";
}

pub type Caller = app_web::Caller<AssistantGate>;

/// The action each operation performs, as an admin granting it sees it.
pub const OPERATIONS: &[Performs] = &[Performs::new("get_assistant_status", "assistant", "chat")];
