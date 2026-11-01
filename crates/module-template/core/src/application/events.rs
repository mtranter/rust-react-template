use super::{PendingEvent, Version};
use crate::domain::{Widget, WidgetEvent};
use widgets_api::events::v1::{self, WidgetTopic};

/// Where the domain's events meet the published contract.
pub(super) fn pending(widget: &Widget, version: Version, event: WidgetEvent) -> PendingEvent {
    let event = match event {
        WidgetEvent::Created => v1::WidgetEvent::Created,
        WidgetEvent::Renamed => v1::WidgetEvent::Renamed,
    };
    PendingEvent::on::<WidgetTopic>(version, &state(widget), event)
}

fn state(widget: &Widget) -> v1::Widget {
    v1::Widget {
        id: widget.id().as_uuid(),
        name: widget.name().to_string(),
    }
}
