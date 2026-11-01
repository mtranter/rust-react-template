//! What `users` announces (spec §4): one topic per aggregate root, each event
//! carrying the root as it now is.

pub mod v1 {
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    /// What every users event carries: the user's state but its audit fields.
    /// Only gains fields, each `#[serde(default)]`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct User {
        pub id: Uuid,
        pub email: String,
        pub display_name: String,
    }

    app_events::topic! {
        /// Everything that happens to a user, keyed by their id.
        pub topic UserTopic("users.user.v1") {
            source: "/users",
            entity: User,
            key: |user| user.id.to_string(),
            events: UserEvent, UserEventKind;
            UserRegistered => Registered: "users.user_registered",
            UserDisplayNameChanged => DisplayNameChanged: "users.user_display_name_changed",
            /// The account is gone, as it was last: its id now names nobody, and
            /// any module holding it decides what that means for its own data.
            UserDeleted => Deleted: "users.user_deleted",
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn every_event_round_trips() {
            let events = [
                UserEvent::Registered,
                UserEvent::DisplayNameChanged,
                UserEvent::Deleted,
            ];
            for event in &events {
                let (event_type, context) = event.encode().unwrap();
                assert_eq!(&UserEvent::decode(event_type, context).unwrap(), event);
            }
            assert_eq!(events.len(), UserEventKind::ALL.len());
        }
    }
}
