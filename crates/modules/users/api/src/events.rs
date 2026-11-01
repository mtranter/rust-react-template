//! What `users` announces (spec §4): one event per state change, each carrying
//! the user as they now are. Stored as JSON, so a version only gains fields,
//! each `#[serde(default)]`.

pub mod v1 {
    use app_events::{Event, NoContext};
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    const SOURCE: &str = "/users";

    /// What every users event carries: the user's state but its audit fields.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct User {
        pub id: Uuid,
        pub email: String,
        pub display_name: String,
    }

    pub struct UserRegistered;

    impl Event for UserRegistered {
        const TYPE: &'static str = "users.user_registered.v1";
        const SOURCE: &'static str = SOURCE;
        type Entity = User;
        type Context = NoContext;

        fn subject(user: &User) -> String {
            user.id.to_string()
        }
    }

    pub struct UserDisplayNameChanged;

    impl Event for UserDisplayNameChanged {
        const TYPE: &'static str = "users.user_display_name_changed.v1";
        const SOURCE: &'static str = SOURCE;
        type Entity = User;
        type Context = NoContext;

        fn subject(user: &User) -> String {
            user.id.to_string()
        }
    }

    /// The account is gone, as it was last: its id now names nobody, and any
    /// module holding it decides what that means for its own data.
    pub struct UserDeleted;

    impl Event for UserDeleted {
        const TYPE: &'static str = "users.user_deleted.v1";
        const SOURCE: &'static str = SOURCE;
        type Entity = User;
        type Context = NoContext;

        fn subject(user: &User) -> String {
            user.id.to_string()
        }
    }
}
