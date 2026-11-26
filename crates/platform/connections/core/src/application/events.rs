use super::{PendingEvent, Version};
use crate::domain::{Connection, ConnectionEvent};
use connections_api::events::v1::{self, ConnectionTopic, OfSecret};

/// Where the domain's events meet the published contract.
pub(crate) fn pending(
    connection: &Connection,
    version: Version,
    event: ConnectionEvent,
) -> PendingEvent {
    let event = match event {
        ConnectionEvent::Declared => v1::ConnectionEvent::Declared,
        ConnectionEvent::Redeclared => v1::ConnectionEvent::Redeclared,
        ConnectionEvent::Retired => v1::ConnectionEvent::Retired,
        ConnectionEvent::Updated => v1::ConnectionEvent::Updated,
        ConnectionEvent::SecretSet { name } => v1::ConnectionEvent::SecretSet(OfSecret { name }),
        ConnectionEvent::SecretCleared { name } => {
            v1::ConnectionEvent::SecretCleared(OfSecret { name })
        }
    };
    PendingEvent::on::<ConnectionTopic>(version, &state(connection), event)
}

/// Never a secret's value, sealed or not.
fn state(connection: &Connection) -> v1::Connection {
    v1::Connection {
        key: connection.key().to_string(),
        label: connection.label().to_owned(),
        declared: connection.declared(),
        url: connection.url().map(str::to_owned),
        fields: connection.fields().clone(),
        secrets: connection
            .secrets()
            .iter()
            .map(|(name, secret)| v1::Secret {
                name: name.clone(),
                set_at: secret.set_at,
                set_by: secret.set_by.clone(),
            })
            .collect(),
    }
}
