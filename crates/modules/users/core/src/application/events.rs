use super::PendingEvent;
use crate::domain::{User, UserEvent};
use app_events::EventPayload;
use users_api::events::v1::{self, UserDeleted, UserDisplayNameChanged, UserRegistered};

/// Where the domain's events meet the published contract.
pub(super) fn pending(user: &User, event: UserEvent) -> PendingEvent {
    let payload = EventPayload::new(state(user));
    match event {
        UserEvent::Registered => PendingEvent::of::<UserRegistered>(payload),
        UserEvent::DisplayNameChanged => PendingEvent::of::<UserDisplayNameChanged>(payload),
        UserEvent::Deleted => PendingEvent::of::<UserDeleted>(payload),
    }
}

fn state(user: &User) -> v1::User {
    v1::User {
        id: user.id().as_uuid(),
        email: user.email().to_string(),
        display_name: user.display_name().to_string(),
    }
}
