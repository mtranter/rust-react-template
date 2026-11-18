use app_web::{Gate, Performs};

/// The users module's gate, `module:users` / `use` (ADR 0040).
pub struct Users;

impl Gate for Users {
    const MODULE: &'static str = "users";
}

/// Who is calling a users operation, once the gate has let them in.
pub type Caller = app_web::Caller<Users>;

/// The action each guarded operation performs, as an admin granting it sees
/// it. `register_user` is public (`server::PUBLIC`).
pub const OPERATIONS: &[Performs] = &[
    Performs::new("get_me", "user", "view"),
    Performs::new("rename_me", "user", "rename"),
    Performs::new("delete_me", "user", "delete"),
    Performs::new("get_user", "user", "view"),
    Performs::new("list_users", "user", "list"),
];
