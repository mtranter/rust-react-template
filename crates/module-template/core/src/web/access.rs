use app_web::{Gate, Performs};

/// The widgets module's gate, `module:widgets` / `use` (ADR 0040).
pub struct Widgets;

impl Gate for Widgets {
    const MODULE: &'static str = "widgets";
}

/// Who is calling a widgets operation, once the gate has let them in.
pub type Caller = app_web::Caller<Widgets>;

/// The action each operation performs, as an admin granting it sees it.
pub const OPERATIONS: &[Performs] = &[
    Performs::new("create_widget", "widget", "create"),
    Performs::new("get_widget", "widget", "view"),
    Performs::new("rename_widget", "widget", "rename"),
    Performs::new("list_widgets", "widget", "list"),
    Performs::new("search_widgets", "widget", "search"),
];
