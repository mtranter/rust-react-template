use uuid::Uuid;

app_domain::uuid_newtype!(WidgetId);

impl WidgetId {
    #[allow(clippy::new_without_default)] // a random `default()` would be misleading
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
