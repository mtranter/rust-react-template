/// What happened to a [`Widget`](super::Widget), as its transitions decide; the
/// service announces each one as the matching `widgets_api` event.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetEvent {
    Created,
    Renamed,
}
