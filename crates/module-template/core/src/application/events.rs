use crate::domain::Widget;
use widgets_api::events::v1;

pub(super) fn widget(widget: &Widget) -> v1::Widget {
    v1::Widget {
        id: widget.id().as_uuid(),
        name: widget.name().to_string(),
    }
}
