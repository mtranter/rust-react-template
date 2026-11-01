use uuid::Uuid;

app_domain::uuid_newtype!(
    /// Private to this module: other modules refer to users by plain `Uuid` (spec D7).
    UserId
);

impl UserId {
    #[allow(clippy::new_without_default)] // a random `default()` would be misleading
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
