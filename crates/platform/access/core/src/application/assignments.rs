use super::service::acting;
use super::{AccessService, Version, Versioned, events};
use crate::domain::{AssignRole, Assignment, AssignmentAction, AssignmentError};
use access_api::RoleAssignments;
use app_authz::{ADMIN, Principal};
use app_error::{AppError, AppResult};

impl AccessService {
    /// Refused for `user`, which every subject holds, and for a role that doesn't exist; a role
    /// assigned to the subject already is a conflict. Bumps the revision, so it takes effect on
    /// each server instance at its next poll.
    pub async fn assign_role(
        &self,
        principal: &Principal,
        cmd: AssignRole,
    ) -> AppResult<Versioned<Assignment>> {
        self.authorizer
            .authorize::<Assignment>(principal, AssignmentAction::Assign)?;
        let (assignment, event) = Assignment::assign(cmd, acting(principal), self.clock.now())?;
        let mut tx = self.transactions.begin().await?;
        let version = self.assignments.insert(&mut tx, &assignment).await?;
        self.publisher
            .publish(
                &mut tx,
                events::assignment_pending(&assignment, version, event),
            )
            .await?;
        self.repo.bump_revision(&mut tx).await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: assignment,
            version,
        })
    }

    /// Refused for `user`; for a bootstrap admin's `admin`, which they hold whatever is assigned;
    /// and for the last stored `admin` while there are no bootstrap admins, so admins can't lock
    /// themselves out.
    pub async fn unassign_role(
        &self,
        principal: &Principal,
        subject: &str,
        role: &str,
        expected: Version,
    ) -> AppResult<()> {
        let plan = self
            .authorizer
            .authorize::<Assignment>(principal, AssignmentAction::Unassign)?;
        let role = Assignment::role_of(role)?;
        let subject = Assignment::subject_of(subject)?;
        let admin = role == ADMIN;
        if admin && self.authorizer.is_bootstrap_admin(subject) {
            return Err(AssignmentError::BootstrapAdmin.into());
        }
        let current = self
            .assignments
            .get(subject, role)
            .await?
            .ok_or_else(|| AppError::not_found("assignment"))?;
        self.authorizer.check(principal, &plan, &current.entity)?;
        if current.version != expected {
            return Err(AppError::precondition_failed(
                "the role was unassigned and assigned again meanwhile",
            ));
        }
        let event = current.entity.unassign(acting(principal));
        let mut tx = self.transactions.begin().await?;
        // Taken before counting, so two removals of `admin` can't each leave the other's as the last.
        self.repo.bump_revision(&mut tx).await?;
        if admin
            && self.authorizer.bootstrap_admins().is_empty()
            && self.assignments.holders(&mut tx, ADMIN).await? <= 1
        {
            return Err(AssignmentError::LastAdmin.into());
        }
        self.assignments
            .delete(&mut tx, subject, role, expected)
            .await?;
        self.publisher
            .publish(
                &mut tx,
                events::assignment_pending(&current.entity, expected.next(), event),
            )
            .await?;
        tx.commit().await
    }
}

#[async_trait::async_trait]
impl RoleAssignments for AccessService {
    async fn ensure_assigned(
        &self,
        principal: &Principal,
        subject: &str,
        role: &str,
    ) -> AppResult<bool> {
        let cmd = AssignRole {
            subject: subject.to_owned(),
            role: role.to_owned(),
        };
        match self.assign_role(principal, cmd).await {
            Ok(_) => Ok(true),
            Err(AppError::Conflict(_)) => Ok(false),
            Err(err) => Err(err),
        }
    }
}
