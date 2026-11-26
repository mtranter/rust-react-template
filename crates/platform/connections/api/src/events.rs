//! What `connections` announces (ADR 0012): the outbox is the Connections' audit trail
//! (ADR 0050), so it carries who set each secret and when, but never a secret's value.

pub mod v1 {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    /// What every connections event carries: the Connection as it now is, its secrets only as
    /// whether and when they were set. Only gains fields, each `#[serde(default)]`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Connection {
        pub key: String,
        pub label: String,
        /// `false` once no module declares it any more.
        pub declared: bool,
        pub url: Option<String>,
        pub fields: BTreeMap<String, String>,
        /// The secrets set, by name.
        pub secrets: Vec<Secret>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Secret {
        pub name: String,
        pub set_at: DateTime<Utc>,
        /// The subject who set it.
        pub set_by: String,
    }

    /// Which secret an event is about: a cleared one is gone from the state.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct OfSecret {
        pub name: String,
    }

    app_events::topic! {
        /// Everything that happens to a Connection, keyed by its key.
        pub topic ConnectionTopic("connections.connection.v1") {
            source: "/connections",
            retention: app_events::TimeDelta::days(30),
            entity: Connection,
            key: |connection| connection.key.clone(),
            events: ConnectionEvent, ConnectionEventKind;
            ConnectionDeclared => Declared: "connections.connection_declared",
            ConnectionRedeclared => Redeclared: "connections.connection_redeclared",
            ConnectionRetired => Retired: "connections.connection_retired",
            ConnectionUpdated => Updated: "connections.connection_updated",
            ConnectionSecretSet => SecretSet(OfSecret): "connections.connection_secret_set",
            ConnectionSecretCleared => SecretCleared(OfSecret): "connections.connection_secret_cleared",
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn every_event_round_trips() {
            let secret = || OfSecret {
                name: "token".into(),
            };
            let events = [
                ConnectionEvent::Declared,
                ConnectionEvent::Redeclared,
                ConnectionEvent::Retired,
                ConnectionEvent::Updated,
                ConnectionEvent::SecretSet(secret()),
                ConnectionEvent::SecretCleared(secret()),
            ];
            for event in &events {
                let (event_type, context) = event.encode().unwrap();
                assert_eq!(
                    &ConnectionEvent::decode(event_type, context).unwrap(),
                    event
                );
            }
            assert_eq!(events.len(), ConnectionEventKind::ALL.len());
        }
    }
}
