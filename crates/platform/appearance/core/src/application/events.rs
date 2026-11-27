use super::{PendingEvent, Version};
use crate::domain::{Theme, ThemeEvent};
use appearance_api::events::v1::{self, ThemeTopic};

/// Where the domain's events meet the published contract.
pub(crate) fn pending(theme: &Theme, version: Version, event: ThemeEvent) -> PendingEvent {
    let event = match event {
        ThemeEvent::Updated => v1::ThemeEvent::Updated,
    };
    let state = v1::Theme {
        light: theme.light().clone(),
        dark: theme.dark().clone(),
        shared: theme.shared().clone(),
        updated_at: theme.updated_at(),
        updated_by: theme.updated_by().to_owned(),
    };
    PendingEvent::on::<ThemeTopic>(version, &state, event)
}
