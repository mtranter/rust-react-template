use app_web::Performs;

/// The action each operation performs, as an admin granting it sees it.
pub const OPERATIONS: &[Performs] = &[
    Performs::new("create_widget", "widget", "create"),
    Performs::new("get_widget", "widget", "view"),
    Performs::new("rename_widget", "widget", "rename"),
    Performs::new("list_widgets", "widget", "list"),
    Performs::new("search_widgets", "widget", "search"),
];
