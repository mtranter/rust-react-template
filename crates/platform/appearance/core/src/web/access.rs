use app_web::Performs;

/// The action each operation performs, as an admin granting it sees it. `get_theme` is public
/// (`server::PUBLIC`), so it performs none.
pub const OPERATIONS: &[Performs] = &[Performs::new("update_theme", "theme", "update")];
