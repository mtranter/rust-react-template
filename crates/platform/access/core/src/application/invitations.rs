use super::service::acting;
use super::{
    AccessService, Created, DirectoryStatus, DirectoryUser, IdentityDirectory, InvitationFilter,
    InvitationSortKey, ListQuery, Listed, Version, Versioned, events,
};
use crate::domain::{
    AssignRole, Assignment, Invitation, InvitationAction, InvitationError, InvitationId,
    InvitationStatus, Invite, InviteUser,
};
use app_authz::Principal;
use app_error::{AppError, AppResult};
use app_persistence::Expected;
use futures_util::future::join_all;
use std::sync::Arc;

/// An invitation as an admin lists it, with where its invitee is in the identity provider:
/// `None` once it isn't pending, or when the provider couldn't say.
#[derive(Debug, Clone)]
pub struct InvitationListing {
    pub invitation: Versioned<Invitation>,
    pub directory: Option<DirectoryStatus>,
}

impl AccessService {
    fn directory(&self) -> AppResult<&Arc<dyn IdentityDirectory>> {
        self.directory.as_ref().ok_or_else(|| {
            AppError::not_configured(
                "inviting is off: no identity provider is configured (COGNITO_USER_POOL_ID)",
            )
        })
    }

    /// Has the identity provider create the invitee, who it mails, then assigns them `roles`
    /// and records them as a subject, in one unit of work. A user the provider has already is
    /// refused, unless they have yet to set a password and no invitation names them: an
    /// invitation whose unit of work failed after the provider created them, which this adopts.
    pub async fn invite(
        &self,
        principal: &Principal,
        cmd: InviteUser,
    ) -> AppResult<Versioned<Invitation>> {
        self.authorizer
            .authorize::<Invitation>(principal, InvitationAction::Invite)?;
        let directory = self.directory()?;
        let invite = Invite::parse(cmd)?;
        for role in &invite.roles {
            if self.roles.get(role).await?.is_none() {
                return Err(AppError::invalid_field(
                    "roles",
                    format!("no role is named `{role}`: create it first"),
                ));
            }
        }
        if self.invitations.pending_for(&invite.email).await?.is_some() {
            return Err(InvitationError::AlreadyPending.into());
        }
        let subject = match directory.create(&invite.email, &invite.name).await? {
            Created::Created(user) => user.subject,
            Created::Exists(DirectoryUser {
                subject,
                status: DirectoryStatus::Pending | DirectoryStatus::Expired,
            }) => {
                tracing::info!("adopting a pending identity-provider user no invitation names");
                subject
            }
            Created::Exists(_) => return Err(InvitationError::AlreadyActive.into()),
        };
        let by = acting(principal);
        let now = self.clock.now();
        let (invitation, event) =
            Invitation::invite(InvitationId::new(), invite, subject, by.clone(), now);

        let mut tx = self.transactions.begin().await?;
        let version = self
            .invitations
            .save(&mut tx, &invitation, Expected::New)
            .await?;
        for role in invitation.roles() {
            let cmd = AssignRole {
                subject: invitation.subject().to_owned(),
                role: role.clone(),
            };
            let (assignment, assigned) =
                Assignment::assign(cmd, by.clone(), now).map_err(InvitationError::Role)?;
            let version = self.assignments.insert(&mut tx, &assignment).await?;
            self.publisher
                .publish(
                    &mut tx,
                    events::assignment_pending(&assignment, version, assigned),
                )
                .await?;
        }
        let name = Some(invitation.name()).filter(|n| !n.is_empty());
        self.subjects
            .introduce(
                &mut tx,
                invitation.subject(),
                name,
                invitation.email().as_str(),
            )
            .await?;
        self.publisher
            .publish(
                &mut tx,
                events::invitation_pending(&invitation, version, event),
            )
            .await?;
        self.repo.bump_revision(&mut tx).await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: invitation,
            version,
        })
    }

    pub async fn list_invitations(
        &self,
        principal: &Principal,
        filter: InvitationFilter,
        query: ListQuery<InvitationSortKey>,
    ) -> AppResult<Listed<InvitationListing>> {
        let rows = self
            .authorizer
            .rows::<Invitation>(principal, InvitationAction::List)?;
        let listed = self.invitations.list(&filter, &query, &rows).await?;
        let statuses = join_all(
            listed
                .items
                .iter()
                .map(|i| self.directory_status(&i.entity)),
        )
        .await;
        Ok(Listed {
            items: listed
                .items
                .into_iter()
                .zip(statuses)
                .map(|(invitation, directory)| InvitationListing {
                    invitation,
                    directory,
                })
                .collect(),
            total: listed.total,
        })
    }

    async fn directory_status(&self, invitation: &Invitation) -> Option<DirectoryStatus> {
        if invitation.status() != InvitationStatus::Pending {
            return None;
        }
        let directory = self.directory.as_ref()?;
        match directory.find(invitation.email()).await {
            Ok(user) => Some(user.status),
            Err(err) => {
                // The list is still worth showing without it.
                tracing::warn!(error = %err, "an invitee's status went unread");
                None
            }
        }
    }

    async fn load_invitation(
        &self,
        principal: &Principal,
        action: InvitationAction,
        id: InvitationId,
        expected: Version,
    ) -> AppResult<Invitation> {
        let plan = self.authorizer.authorize::<Invitation>(principal, action)?;
        let current = self
            .invitations
            .get(id)
            .await?
            .ok_or_else(|| AppError::not_found("invitation"))?;
        self.authorizer.check(principal, &plan, &current.entity)?;
        if current.version != expected {
            return Err(AppError::precondition_failed(
                "the invitation changed meanwhile",
            ));
        }
        Ok(current.entity)
    }

    /// Has the identity provider mail a pending invitee a fresh temporary password.
    pub async fn resend_invitation(
        &self,
        principal: &Principal,
        id: InvitationId,
        expected: Version,
    ) -> AppResult<Versioned<Invitation>> {
        let directory = self.directory()?.clone();
        let invitation = self
            .load_invitation(principal, InvitationAction::Resend, id, expected)
            .await?;
        let event = invitation.resend(acting(principal))?;
        match directory.find(invitation.email()).await?.status {
            DirectoryStatus::Active => return Err(InvitationError::PasswordSet.into()),
            DirectoryStatus::Missing => {
                return Err(AppError::conflict(
                    "the identity provider no longer has the invitee: revoke the invitation and invite them again",
                ));
            }
            DirectoryStatus::Pending | DirectoryStatus::Expired => {}
        }
        directory.resend(invitation.email()).await?;
        let mut tx = self.transactions.begin().await?;
        let version = self
            .invitations
            .save(&mut tx, &invitation, Expected::Version(expected))
            .await?;
        self.publisher
            .publish(
                &mut tx,
                events::invitation_pending(&invitation, version, event),
            )
            .await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: invitation,
            version,
        })
    }

    /// Deletes a pending invitee from the identity provider and unassigns the invitation's
    /// roles. Refused once they have set their password, as they may be signed in.
    pub async fn revoke_invitation(
        &self,
        principal: &Principal,
        id: InvitationId,
        expected: Version,
    ) -> AppResult<()> {
        let directory = self.directory()?.clone();
        let mut invitation = self
            .load_invitation(principal, InvitationAction::Revoke, id, expected)
            .await?;
        let by = acting(principal);
        let event = invitation.revoke(by.clone())?;
        if directory.find(invitation.email()).await?.status == DirectoryStatus::Active {
            return Err(InvitationError::PasswordSet.into());
        }
        directory.delete(invitation.email()).await?;
        let mut tx = self.transactions.begin().await?;
        self.repo.bump_revision(&mut tx).await?;
        let version = self
            .invitations
            .save(&mut tx, &invitation, Expected::Version(expected))
            .await?;
        for role in invitation.roles() {
            // Unassigned meanwhile by an admin: nothing left to remove.
            let Some(current) = self.assignments.get(invitation.subject(), role).await? else {
                continue;
            };
            self.assignments
                .delete(&mut tx, invitation.subject(), role, current.version)
                .await?;
            self.publisher
                .publish(
                    &mut tx,
                    events::assignment_pending(
                        &current.entity,
                        current.version.next(),
                        current.entity.unassign(by.clone()),
                    ),
                )
                .await?;
        }
        self.publisher
            .publish(
                &mut tx,
                events::invitation_pending(&invitation, version, event),
            )
            .await?;
        tx.commit().await
    }
}
