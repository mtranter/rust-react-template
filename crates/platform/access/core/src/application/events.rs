use super::{PendingEvent, Version};
use crate::domain::{Grant, GrantEvent, Scope};
use access_api::events::v1::{self, GrantTopic};

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

/// `any`, or the scope's name: a scope can't be named `any` (ADR 0040).
pub(crate) fn scope_name(scope: &Scope) -> &str {
    match scope {
        Scope::Any => "any",
        Scope::Named(name) => name,
    }
}
