use super::PendingEvent;
use crate::domain::{Widget, WidgetEvent};
use app_events::EventPayload;
use widgets_api::events::v1::{self, WidgetCreated, WidgetRenamed};

/// Where the domain's events meet the published contract.
pub(super) fn pending(widget: &Widget, event: WidgetEvent) -> PendingEvent {
    let payload = EventPayload::new(state(widget));
    match event {
        WidgetEvent::Created => PendingEvent::of::<WidgetCreated>(payload),
        WidgetEvent::Renamed => PendingEvent::of::<WidgetRenamed>(payload),
    }
}

fn state(widget: &Widget) -> v1::Widget {
    v1::Widget {
        id: widget.id().as_uuid(),
        name: widget.name().to_string(),
    }
}
