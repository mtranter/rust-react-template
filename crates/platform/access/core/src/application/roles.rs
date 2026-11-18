use super::service::acting;
use super::{AccessService, ListQuery, Listed, RoleSortKey, Version, Versioned, events};
use crate::domain::{CreateRole, Role, RoleAction};
use app_authz::{Plan, Principal};
use app_error::{AppError, AppResult};

impl AccessService {
    fn authorize_role(&self, principal: &Principal, action: RoleAction) -> AppResult<Plan> {
        self.authorizer.authorize::<Role>(principal, action)
    }

    pub async fn list_roles(
        &self,
        principal: &Principal,
        query: ListQuery<RoleSortKey>,
    ) -> AppResult<Listed<Versioned<Role>>> {
        let rows = self.authorizer.rows::<Role>(principal, RoleAction::List)?;
        self.roles.list(&query, &rows).await
    }

    /// A name a role has is a conflict. Bumps the revision, as a change to grants does.
    pub async fn create_role(
        &self,
        principal: &Principal,
        cmd: CreateRole,
    ) -> AppResult<Versioned<Role>> {
        self.authorize_role(principal, RoleAction::Create)?;
        let (role, event) = Role::create(cmd, acting(principal), self.clock.now())?;
        let mut tx = self.transactions.begin().await?;
        let version = self.roles.insert(&mut tx, &role).await?;
        self.publisher
            .publish(&mut tx, events::role_pending(&role, version, event))
            .await?;
        self.repo.bump_revision(&mut tx).await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: role,
            version,
        })
    }

    /// Refused for a built-in role, and while a grant names it, so no grant is left naming
    /// nothing: an admin revokes those first.
    pub async fn delete_role(
        &self,
        principal: &Principal,
        name: &str,
        expected: Version,
    ) -> AppResult<()> {
        let plan = self.authorize_role(principal, RoleAction::Delete)?;
        let current = self
            .roles
            .get(name)
            .await?
            .ok_or_else(|| AppError::not_found("role"))?;
        self.authorizer.check(principal, &plan, &current.entity)?;
        if current.version != expected {
            return Err(AppError::precondition_failed(
                "the role was deleted and created again meanwhile",
            ));
        }
        let event = current.entity.delete(acting(principal))?;
        let mut tx = self.transactions.begin().await?;
        self.roles.delete(&mut tx, name, expected).await?;
        self.publisher
            .publish(
                &mut tx,
                events::role_pending(&current.entity, expected.next(), event),
            )
            .await?;
        self.repo.bump_revision(&mut tx).await?;
        tx.commit().await
    }
}
