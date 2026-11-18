use app_web::{Gate, Performs};

/// The access module's gate, `module:access` / `use` (ADR 0040): `admin`'s by its own
/// policies, whatever is granted or revoked.
pub struct Access;

impl Gate for Access {
    const MODULE: &'static str = "access";
}

/// Who is calling an access operation, once the gate has let them in.
pub type Caller = app_web::Caller<Access>;

/// The action each operation performs, as an admin granting it sees it.
pub const OPERATIONS: &[Performs] = &[
    Performs::new("list_grants", "grant", "list"),
    Performs::new("get_access_catalog", "grant", "list"),
    Performs::new("grant_access", "grant", "create"),
    Performs::new("revoke_grant", "grant", "revoke"),
    Performs::new("list_roles", "role", "list"),
    Performs::new("create_role", "role", "create"),
    Performs::new("delete_role", "role", "delete"),
    Performs::new("assign_role", "assignment", "assign"),
    Performs::new("unassign_role", "assignment", "unassign"),
];
