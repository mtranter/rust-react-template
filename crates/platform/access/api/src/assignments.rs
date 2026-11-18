use app_authz::Principal;
use app_error::AppResult;

/// Assigns roles to subjects (the token's `sub`), as an admin's `assign_role` does: a module's
/// seed gives the accounts it seeds their roles through it, as `Principal::System`.
#[async_trait::async_trait]
pub trait RoleAssignments: Send + Sync {
    /// Assigns `role` to `subject` unless it is already, which changes nothing; `false` then.
    /// Refused as `assign_role` is: `user` (which every subject holds) and a role that doesn't
    /// exist are invalid.
    async fn ensure_assigned(
        &self,
        principal: &Principal,
        subject: &str,
        role: &str,
    ) -> AppResult<bool>;
}
