use app_web::{Gate, Performs};

/// The note-search module's gate, `module:note-search` / `use` (ADR 0040).
pub struct NoteSearch;

impl Gate for NoteSearch {
    const MODULE: &'static str = "note-search";
}

/// Who is calling a note-search operation, once the gate has let them in.
pub type Caller = app_web::Caller<NoteSearch>;

/// The action each operation performs, as an admin granting it sees it.
pub const OPERATIONS: &[Performs] = &[Performs::new("search_notes", "noteSummary", "search")];
