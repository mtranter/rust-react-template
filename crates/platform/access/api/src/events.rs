//! What `access` announces (ADR 0012): a topic per grant, each event carrying the grant.

pub mod v1 {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    /// What every grant event carries. Only gains fields, each `#[serde(default)]`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Grant {
        pub id: Uuid,
        /// As the identity provider names it in the token's `roles`.
        pub role: String,
        pub resource: String,
        pub action: String,
        /// `any`, or one of the resource's scopes.
        pub scope: String,
        /// The subject who granted it; absent for a default grant, applied by `migrate`.
        pub granted_by: Option<String>,
        pub granted_at: DateTime<Utc>,
    }

    /// Who revoked the grant: the grant's state can't say.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RevokedBy {
        pub revoked_by: Option<String>,
    }

    app_events::topic! {
        /// Every grant made and revoked, keyed by the grant's id.
        pub topic GrantTopic("access.grant.v1") {
            source: "/access",
            retention: app_events::TimeDelta::days(7),
            entity: Grant,
            key: |grant| grant.id.to_string(),
            events: GrantEvent, GrantEventKind;
            AccessGranted => Granted: "access.access_granted",
            GrantRevoked => Revoked(RevokedBy): "access.grant_revoked",
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn every_event_round_trips() {
            let events = [
                GrantEvent::Granted,
                GrantEvent::Revoked(RevokedBy {
                    revoked_by: Some("someone".into()),
                }),
            ];
            for event in &events {
                let (event_type, context) = event.encode().unwrap();
                assert_eq!(&GrantEvent::decode(event_type, context).unwrap(), event);
            }
            assert_eq!(events.len(), GrantEventKind::ALL.len());
        }
    }
}
