use app_web::Performs;

/// The action each operation performs, as an admin granting it sees it.
pub const OPERATIONS: &[Performs] = &[
    Performs::new("list_grants", "grant", "list"),
    Performs::new("get_access_catalog", "grant", "list"),
    Performs::new("grant_access", "grant", "create"),
    Performs::new("grant_access_in_bulk", "grant", "create"),
    Performs::new("revoke_grant", "grant", "revoke"),
    Performs::new("list_roles", "role", "list"),
    Performs::new("create_role", "role", "create"),
    Performs::new("delete_role", "role", "delete"),
    Performs::new("assign_role", "assignment", "assign"),
    Performs::new("unassign_role", "assignment", "unassign"),
    Performs::new("list_subjects", "subject", "list"),
    Performs::new("invite_user", "invitation", "invite"),
    Performs::new("list_invitations", "invitation", "list"),
    Performs::new("resend_invitation", "invitation", "resend"),
    Performs::new("revoke_invitation", "invitation", "revoke"),
    Performs::new("list_access_audit", "auditEntry", "list"),
];
