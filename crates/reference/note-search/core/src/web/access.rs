use app_web::Performs;

/// The action each operation performs, as an admin granting it sees it.
pub const OPERATIONS: &[Performs] = &[Performs::new("search_notes", "noteSummary", "search")];
