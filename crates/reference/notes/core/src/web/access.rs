use app_web::Performs;

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
