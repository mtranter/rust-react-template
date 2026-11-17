/// What happened to a [`User`](super::User), as its transitions decide; the
/// service announces each one as the matching `users_api` event.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserEvent {
    Registered,
    DisplayNameChanged,
    Deleted,
}
