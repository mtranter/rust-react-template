use app_web::Performs;

/// The action each operation performs, as an admin granting it sees it. `get_theme` is public
/// (`server::PUBLIC`) and `get_app_visibility` any signed-in caller's (`server::SIGNED_IN`), so
/// they perform none.
pub const OPERATIONS: &[Performs] = &[
    Performs::new("update_theme", "theme", "update"),
    Performs::new("update_app_visibility", "appVisibility", "update"),
];
