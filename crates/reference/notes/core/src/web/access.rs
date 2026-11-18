use app_web::{Gate, Performs};

/// The notes module's gate, `module:notes` / `use` (ADR 0040).
pub struct Notes;

impl Gate for Notes {
    const MODULE: &'static str = "notes";
}

/// Who is calling a notes operation, once the gate has let them in.
pub type Caller = app_web::Caller<Notes>;

/// The action each operation performs, as an admin granting it sees it.
pub const OPERATIONS: &[Performs] = &[
    Performs::new("create_note", "note", "create"),
    Performs::new("get_note", "note", "view"),
    Performs::new("edit_note", "note", "edit"),
    Performs::new("delete_note", "note", "delete"),
    Performs::new("add_note_item", "note", "items:add"),
    Performs::new("remove_note_item", "note", "items:remove"),
    Performs::new("list_notes", "note", "list"),
];
