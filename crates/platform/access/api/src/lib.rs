//! What other modules may know of and ask of `access` (ADR 0008, 0040): the topics its grants,
//! roles and assignments are announced on, which are the audit trail of who was granted and
//! assigned what, and by whom; and assigning a role, as a module's seed does for its users.

mod assignments;
pub mod events;

pub use assignments::RoleAssignments;
