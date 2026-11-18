//! What `access` announces (ADR 0012): a topic per root, grants', roles' and assignments', each
//! event carrying the grant, role or assignment.

pub mod v1 {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    /// What every grant event carries. Only gains fields, each `#[serde(default)]`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Grant {
        pub id: Uuid,
        /// The role's name (`access.role.v1`).
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

    /// What every role event carries. Only gains fields, each `#[serde(default)]`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Role {
        /// The role's id, as grants name it.
        pub name: String,
        pub description: String,
        /// The subject who created it; absent for one `migrate` created for a default grant.
        pub created_by: Option<String>,
        pub created_at: DateTime<Utc>,
    }

    /// Who deleted the role: the role's state can't say.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeletedBy {
        pub deleted_by: Option<String>,
    }

    app_events::topic! {
        /// Every role created and deleted, keyed by the role's name.
        pub topic RoleTopic("access.role.v1") {
            source: "/access",
            retention: app_events::TimeDelta::days(7),
            entity: Role,
            key: |role| role.name.clone(),
            events: RoleEvent, RoleEventKind;
            RoleCreated => Created: "access.role_created",
            RoleDeleted => Deleted(DeletedBy): "access.role_deleted",
        }
    }

    /// What every assignment event carries. Only gains fields, each `#[serde(default)]`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Assignment {
        /// Whom the role is assigned to: the token's `sub`.
        pub subject: String,
        pub role: String,
        /// The subject who assigned it; absent for one a seed assigned as `System`.
        pub assigned_by: Option<String>,
        pub assigned_at: DateTime<Utc>,
    }

    /// Who unassigned the role: the assignment's state can't say.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UnassignedBy {
        pub unassigned_by: Option<String>,
    }

    /// The key of an assignment's events: its role and subject, which identify it. The role's
    /// length leads, so no two pairs share a key whatever characters they hold.
    pub fn assignment_key(subject: &str, role: &str) -> String {
        format!("{}:{role}:{subject}", role.len())
    }

    app_events::topic! {
        /// Every role assigned to a subject and unassigned, keyed by the role and subject.
        pub topic AssignmentTopic("access.assignment.v1") {
            source: "/access",
            retention: app_events::TimeDelta::days(7),
            entity: Assignment,
            key: |assignment| assignment_key(&assignment.subject, &assignment.role),
            events: AssignmentEvent, AssignmentEventKind;
            RoleAssigned => Assigned: "access.role_assigned",
            RoleUnassigned => Unassigned(UnassignedBy): "access.role_unassigned",
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

        #[test]
        fn every_role_event_round_trips() {
            let events = [
                RoleEvent::Created,
                RoleEvent::Deleted(DeletedBy {
                    deleted_by: Some("someone".into()),
                }),
            ];
            for event in &events {
                let (event_type, context) = event.encode().unwrap();
                assert_eq!(&RoleEvent::decode(event_type, context).unwrap(), event);
            }
            assert_eq!(events.len(), RoleEventKind::ALL.len());
        }

        #[test]
        fn every_assignment_event_round_trips() {
            let events = [
                AssignmentEvent::Assigned,
                AssignmentEvent::Unassigned(UnassignedBy {
                    unassigned_by: Some("someone".into()),
                }),
            ];
            for event in &events {
                let (event_type, context) = event.encode().unwrap();
                assert_eq!(
                    &AssignmentEvent::decode(event_type, context).unwrap(),
                    event
                );
            }
            assert_eq!(events.len(), AssignmentEventKind::ALL.len());
        }
    }
}
