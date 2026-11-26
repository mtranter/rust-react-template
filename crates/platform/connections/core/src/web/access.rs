use app_web::Performs;

/// The action each operation performs, as an admin granting it sees it.
pub const OPERATIONS: &[Performs] = &[
    Performs::new("list_connections", "connection", "list"),
    Performs::new("get_connection", "connection", "view"),
    Performs::new("update_connection", "connection", "edit"),
    Performs::new("set_connection_secret", "connection", "setSecret"),
    Performs::new("clear_connection_secret", "connection", "clearSecret"),
];
