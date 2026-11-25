use uuid::Uuid;

app_domain::uuid_newtype!(GrantId);

impl GrantId {
    #[allow(clippy::new_without_default)] // a random `default()` would be misleading
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

app_domain::uuid_newtype!(InvitationId);

impl InvitationId {
    #[allow(clippy::new_without_default)] // a random `default()` would be misleading
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
