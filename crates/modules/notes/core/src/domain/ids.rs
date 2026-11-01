use uuid::Uuid;

app_domain::uuid_newtype!(NoteId);

impl NoteId {
    #[allow(clippy::new_without_default)] // a random `default()` would be misleading
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

app_domain::uuid_newtype!(
    /// Notes' own type so the domain names no other module (spec D7). No `new()`:
    /// notes never creates users.
    AuthorId
);
