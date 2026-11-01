//! What `users` announces (spec §4). Other modules subscribe to these; they're
//! stored as JSON, so a version only gains fields, each `#[serde(default)]`.

pub mod v1 {
    use app_persistence::Event;
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    /// The account is gone: its id now names nobody, and any module holding it
    /// decides what that means for its own data.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserDeleted {
        pub user_id: Uuid,
    }

    impl Event for UserDeleted {
        const TYPE: &'static str = "users.user_deleted.v1";
    }
}
