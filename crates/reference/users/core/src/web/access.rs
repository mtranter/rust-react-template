use app_web::Performs;

/// The action each guarded operation performs, as an admin granting it sees
/// it. `register_user` is public (`server::PUBLIC`).
pub const OPERATIONS: &[Performs] = &[
    Performs::new("get_me", "user", "view"),
    Performs::new("rename_me", "user", "rename"),
    Performs::new("delete_me", "user", "delete"),
    Performs::new("get_user", "user", "view"),
    Performs::new("list_users", "user", "list"),
];
