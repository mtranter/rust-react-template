/// What happened to the [`Theme`](super::Theme); the service announces it as the matching
/// `appearance_api` event.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeEvent {
    Updated,
}

/// What happened to the [`AppVisibility`](super::AppVisibility).
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppVisibilityEvent {
    Updated,
}
