/// What happened to a [`Connection`](super::Connection), as its transitions decide; the service
/// announces each one as the matching `connections_api` event.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionEvent {
    Declared,
    Redeclared,
    Retired,
    Updated,
    SecretSet { name: String },
    SecretCleared { name: String },
}
