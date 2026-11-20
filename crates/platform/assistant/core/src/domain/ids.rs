use uuid::Uuid;

app_domain::uuid_newtype!(ConversationId);

impl ConversationId {
    #[allow(clippy::new_without_default)] // a random `default()` would be misleading
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

app_domain::uuid_newtype!(MessageId);

impl MessageId {
    #[allow(clippy::new_without_default)] // a random `default()` would be misleading
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

app_domain::uuid_newtype!(ToolCallId);

impl ToolCallId {
    #[allow(clippy::new_without_default)] // a random `default()` would be misleading
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
