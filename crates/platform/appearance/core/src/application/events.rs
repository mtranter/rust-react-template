use super::{PendingEvent, Version};
use crate::domain::{AppVisibility, AppVisibilityEvent, Theme, ThemeEvent};
use appearance_api::events::v1::{self, AppVisibilityTopic, ThemeTopic};

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

pub(crate) fn app_visibility_pending(
    visibility: &AppVisibility,
    version: Version,
    event: AppVisibilityEvent,
) -> PendingEvent {
    let event = match event {
        AppVisibilityEvent::Updated => v1::AppVisibilityEvent::Updated,
    };
    let state = v1::AppVisibility {
        hidden: visibility.hidden().iter().cloned().collect(),
        updated_at: visibility.updated_at(),
        updated_by: visibility.updated_by().to_owned(),
    };
    PendingEvent::on::<AppVisibilityTopic>(version, &state, event)
}
