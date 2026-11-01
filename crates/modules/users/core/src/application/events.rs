use super::{PendingEvent, Version};
use crate::domain::{User, UserEvent};
use users_api::events::v1::{self, UserTopic};

/// Where the domain's events meet the published contract.
pub(super) fn pending(user: &User, version: Version, event: UserEvent) -> PendingEvent {
    let event = match event {
        UserEvent::Registered => v1::UserEvent::Registered,
        UserEvent::DisplayNameChanged => v1::UserEvent::DisplayNameChanged,
        UserEvent::Deleted => v1::UserEvent::Deleted,
    };
    PendingEvent::on::<UserTopic>(version, &state(user), event)
}

fn state(user: &User) -> v1::User {
    v1::User {
        id: user.id().as_uuid(),
        email: user.email().to_string(),
        display_name: user.display_name().to_string(),
    }
}
