use super::{PendingEvent, Version};
use crate::domain::{
    Assignment, AssignmentEvent, Grant, GrantEvent, Invitation, InvitationEvent, Role, RoleEvent,
    Scope,
};
use access_api::events::v1::{self, AssignmentTopic, GrantTopic, InvitationTopic, RoleTopic};

/// Where the domain's events meet the published contract.
pub(crate) fn pending(grant: &Grant, version: Version, event: GrantEvent) -> PendingEvent {
    let event = match event {
        GrantEvent::Granted => v1::GrantEvent::Granted,
        GrantEvent::Revoked { by } => v1::GrantEvent::Revoked(v1::RevokedBy {
            revoked_by: by.subject().map(str::to_owned),
        }),
    };
    PendingEvent::on::<GrantTopic>(version, &state(grant), event)
}

fn state(grant: &Grant) -> v1::Grant {
    v1::Grant {
        id: grant.id().as_uuid(),
        role: grant.role().to_owned(),
        resource: grant.resource().to_owned(),
        action: grant.action().to_owned(),
        scope: scope_name(grant.scope()).to_owned(),
        granted_by: grant.granted_by().subject().map(str::to_owned),
        granted_at: grant.granted_at(),
    }
}

pub(crate) fn role_pending(role: &Role, version: Version, event: RoleEvent) -> PendingEvent {
    let event = match event {
        RoleEvent::Created => v1::RoleEvent::Created,
        RoleEvent::Deleted { by } => v1::RoleEvent::Deleted(v1::DeletedBy {
            deleted_by: by.subject().map(str::to_owned),
        }),
    };
    let state = v1::Role {
        name: role.name().to_owned(),
        description: role.description().to_owned(),
        created_by: role.created_by().subject().map(str::to_owned),
        created_at: role.created_at(),
    };
    PendingEvent::on::<RoleTopic>(version, &state, event)
}

pub(crate) fn assignment_pending(
    assignment: &Assignment,
    version: Version,
    event: AssignmentEvent,
) -> PendingEvent {
    let event = match event {
        AssignmentEvent::Assigned => v1::AssignmentEvent::Assigned,
        AssignmentEvent::Unassigned { by } => v1::AssignmentEvent::Unassigned(v1::UnassignedBy {
            unassigned_by: by.subject().map(str::to_owned),
        }),
    };
    let state = v1::Assignment {
        subject: assignment.subject().to_owned(),
        role: assignment.role().to_owned(),
        assigned_by: assignment.assigned_by().subject().map(str::to_owned),
        assigned_at: assignment.assigned_at(),
    };
    PendingEvent::on::<AssignmentTopic>(version, &state, event)
}

/// `any`, or the scope's name: a scope can't be named `any` (ADR 0040).
pub(crate) fn scope_name(scope: &Scope) -> &str {
    match scope {
        Scope::Any => "any",
        Scope::Named(name) => name,
    }
}

pub(crate) fn invitation_pending(
    invitation: &Invitation,
    version: Version,
    event: InvitationEvent,
) -> PendingEvent {
    let event = match event {
        InvitationEvent::Created => v1::InvitationEvent::Created,
        InvitationEvent::Resent { by } => v1::InvitationEvent::Resent(v1::ResentBy {
            resent_by: by.subject().map(str::to_owned),
        }),
        InvitationEvent::Accepted => v1::InvitationEvent::Accepted,
        InvitationEvent::Revoked { by } => v1::InvitationEvent::Revoked(v1::RevokedInvitationBy {
            revoked_by: by.subject().map(str::to_owned),
        }),
    };
    let status: &'static str = invitation.status().into();
    let state = v1::Invitation {
        id: invitation.id().as_uuid(),
        email: invitation.email().as_str().to_owned(),
        name: invitation.name().to_owned(),
        subject: invitation.subject().to_owned(),
        roles: invitation.roles().to_vec(),
        invited_by: invitation.invited_by().subject().map(str::to_owned),
        invited_at: invitation.invited_at(),
        status: status.to_owned(),
    };
    PendingEvent::on::<InvitationTopic>(version, &state, event)
}
