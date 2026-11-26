use super::{ConnectionError, ConnectionEvent, ConnectionKey, Secret, UpdateConnection};
use connections_api::{Auth, Declaration};
use std::collections::{BTreeMap, BTreeSet};

const MAX_URL_LEN: usize = 2000;
const MAX_FIELD_LEN: usize = 2000;

/// A Connection to another system (ADR 0050): what code declares of it, and the URL, fields and
/// sealed secrets an admin gives it. Its secrets are children: each is saved with the root's
/// version guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    key: ConnectionKey,
    declaration: Declaration,
    declared: bool,
    url: Option<String>,
    fields: BTreeMap<String, String>,
    secrets: BTreeMap<String, Secret>,
}

impl Connection {
    /// A Connection a module has declared for the first time, with no values yet.
    pub fn declare(declaration: Declaration) -> Result<(Self, ConnectionEvent), ConnectionError> {
        let key = validated(&declaration)?;
        let connection = Self {
            key,
            declaration,
            declared: true,
            url: None,
            fields: BTreeMap::new(),
            secrets: BTreeMap::new(),
        };
        Ok((connection, ConnectionEvent::Declared))
    }

    /// As a module now declares it; `None` when that changes nothing. Values the declaration no
    /// longer names (a field removed, a secret of another auth style) are dropped.
    pub fn redeclare(
        &mut self,
        declaration: Declaration,
    ) -> Result<Option<ConnectionEvent>, ConnectionError> {
        validated(&declaration)?;
        if self.declared && self.declaration == declaration {
            return Ok(None);
        }
        self.declaration = declaration;
        self.declared = true;
        let fields: BTreeSet<String> = self.field_names().map(str::to_owned).collect();
        self.fields.retain(|name, _| fields.contains(name));
        let secrets = self.declaration.secrets();
        self.secrets
            .retain(|name, _| secrets.iter().any(|(secret, _)| secret == name));
        Ok(Some(ConnectionEvent::Redeclared))
    }

    /// No module declares it any more; `None` when it was already retired. Its values stay, so
    /// declaring it again finds them.
    pub fn retire(&mut self) -> Option<ConnectionEvent> {
        if !self.declared {
            return None;
        }
        self.declared = false;
        Some(ConnectionEvent::Retired)
    }

    /// On error the Connection is left unchanged.
    pub fn update(&mut self, cmd: UpdateConnection) -> Result<ConnectionEvent, ConnectionError> {
        let url = match cmd.url {
            Some(url) => url.map(|raw| valid_url(&raw)).transpose()?.flatten(),
            None => self.url.clone(),
        };
        let mut fields = self.fields.clone();
        for (name, value) in cmd.fields {
            if !self.field_names().any(|declared| declared == name) {
                return Err(ConnectionError::UnknownField(name));
            }
            let value = value.trim();
            if value.len() > MAX_FIELD_LEN {
                return Err(ConnectionError::FieldTooLong {
                    name,
                    max: MAX_FIELD_LEN,
                });
            }
            if value.is_empty() {
                fields.remove(&name);
            } else {
                fields.insert(name, value.to_owned());
            }
        }
        self.url = url;
        self.fields = fields;
        Ok(ConnectionEvent::Updated)
    }

    /// Whether `name` is one of the secrets its auth style takes.
    pub fn takes_secret(&self, name: &str) -> Result<(), ConnectionError> {
        if self.declaration.secrets().iter().any(|(s, _)| *s == name) {
            Ok(())
        } else {
            Err(ConnectionError::UnknownSecret(name.to_owned()))
        }
    }

    /// Replaces the secret `name`, if it had one: how a key is rotated.
    pub fn set_secret(
        &mut self,
        name: &str,
        secret: Secret,
    ) -> Result<ConnectionEvent, ConnectionError> {
        self.takes_secret(name)?;
        self.secrets.insert(name.to_owned(), secret);
        Ok(ConnectionEvent::SecretSet {
            name: name.to_owned(),
        })
    }

    /// `None` when the secret wasn't set.
    pub fn clear_secret(&mut self, name: &str) -> Result<Option<ConnectionEvent>, ConnectionError> {
        self.takes_secret(name)?;
        Ok(self
            .secrets
            .remove(name)
            .map(|_| ConnectionEvent::SecretCleared {
                name: name.to_owned(),
            }))
    }

    /// Skips invariants: for repositories rebuilding persisted state only.
    pub fn restore(
        key: ConnectionKey,
        declaration: Declaration,
        declared: bool,
        url: Option<String>,
        fields: BTreeMap<String, String>,
        secrets: BTreeMap<String, Secret>,
    ) -> Self {
        Self {
            key,
            declaration,
            declared,
            url,
            fields,
            secrets,
        }
    }

    pub fn key(&self) -> &ConnectionKey {
        &self.key
    }

    pub fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    pub fn label(&self) -> &str {
        &self.declaration.label
    }

    pub fn auth(&self) -> &Auth {
        &self.declaration.auth
    }

    pub fn declared(&self) -> bool {
        self.declared
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn fields(&self) -> &BTreeMap<String, String> {
        &self.fields
    }

    pub fn secrets(&self) -> &BTreeMap<String, Secret> {
        &self.secrets
    }

    /// When the secret `name` was last set, if it is.
    pub fn secret(&self, name: &str) -> Option<&Secret> {
        self.secrets.get(name)
    }

    /// What an admin has yet to give it before it can be called: its URL, then each field and
    /// secret its auth style takes, by label.
    pub fn missing(&self) -> Vec<&str> {
        let url = self.url.is_none().then_some("URL");
        let fields = self
            .declaration
            .auth
            .fields()
            .iter()
            .filter(|(name, _)| !self.fields.contains_key(*name))
            .map(|(_, label)| *label);
        let secrets = self
            .declaration
            .secrets()
            .iter()
            .filter(|(name, _)| !self.secrets.contains_key(*name))
            .map(|(_, label)| *label);
        url.into_iter().chain(fields).chain(secrets).collect()
    }

    pub fn configured(&self) -> bool {
        self.missing().is_empty()
    }

    fn field_names(&self) -> impl Iterator<Item = &str> {
        self.declaration
            .all_fields()
            .into_iter()
            .map(|(name, _)| name)
    }
}

fn validated(declaration: &Declaration) -> Result<ConnectionKey, ConnectionError> {
    let key = ConnectionKey::parse(&declaration.key)?;
    if declaration.label.trim().is_empty() {
        return Err(ConnectionError::EmptyLabel(declaration.key.clone()));
    }
    let mut names = BTreeSet::new();
    for (name, label) in declaration.all_fields() {
        if name.trim().is_empty() || label.trim().is_empty() {
            return Err(ConnectionError::UnnamedField {
                key: declaration.key.clone(),
            });
        }
        if !names.insert(name) {
            return Err(ConnectionError::DuplicateField {
                key: declaration.key.clone(),
                name: name.to_owned(),
            });
        }
    }
    if let Auth::ApiKeyHeader { name } | Auth::ApiKeyQuery { name } = &declaration.auth
        && name.trim().is_empty()
    {
        return Err(ConnectionError::UnnamedApiKey(declaration.key.clone()));
    }
    Ok(key)
}

/// `None` for a blank URL, which clears it.
fn valid_url(raw: &str) -> Result<Option<String>, ConnectionError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let invalid = ConnectionError::InvalidUrl { max: MAX_URL_LEN };
    if raw.len() > MAX_URL_LEN {
        return Err(invalid);
    }
    let url = url::Url::parse(raw).map_err(|_| invalid.clone())?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        return Err(invalid);
    }
    Ok(Some(raw.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SealedSecret;
    use chrono::DateTime;

    fn crm() -> Declaration {
        Declaration::new("crm", "CRM")
            .auth(Auth::Basic)
            .field("tenant", "Tenant")
    }

    fn declared() -> Connection {
        Connection::declare(crm()).unwrap().0
    }

    fn secret() -> Secret {
        Secret {
            sealed: SealedSecret {
                ciphertext: vec![1],
                wrapped_key: vec![2],
                key_version: "test".into(),
            },
            set_at: DateTime::UNIX_EPOCH,
            set_by: "admin".into(),
        }
    }

    #[test]
    fn a_declaration_must_be_well_formed() {
        assert_eq!(
            Connection::declare(Declaration::new("CRM", "CRM")).map(|_| ()),
            Err(ConnectionError::InvalidKey("CRM".into()))
        );
        assert_eq!(
            Connection::declare(Declaration::new("crm", " ")).map(|_| ()),
            Err(ConnectionError::EmptyLabel("crm".into()))
        );
        assert_eq!(
            Connection::declare(crm().field("username", "User")).map(|_| ()),
            Err(ConnectionError::DuplicateField {
                key: "crm".into(),
                name: "username".into()
            })
        );
        assert_eq!(
            Connection::declare(
                Declaration::new("crm", "CRM").auth(Auth::ApiKeyQuery { name: "".into() })
            )
            .map(|_| ()),
            Err(ConnectionError::UnnamedApiKey("crm".into()))
        );
    }

    #[test]
    fn an_update_takes_an_http_url_and_declared_fields_only() {
        let mut connection = declared();
        let before = connection.clone();
        for url in ["ftp://crm.example", "crm.example", "https://"] {
            let refused = connection.update(UpdateConnection {
                url: Some(Some(url.into())),
                ..Default::default()
            });
            assert_eq!(
                refused,
                Err(ConnectionError::InvalidUrl { max: MAX_URL_LEN }),
                "{url}"
            );
        }
        let refused = connection.update(UpdateConnection {
            url: Some(Some("https://crm.example".into())),
            fields: [("region".to_owned(), "eu".to_owned())].into(),
        });
        assert_eq!(refused, Err(ConnectionError::UnknownField("region".into())));
        assert_eq!(connection, before);

        let updated = connection.update(UpdateConnection {
            url: Some(Some(" https://crm.example/api ".into())),
            fields: [
                ("tenant".to_owned(), " acme ".to_owned()),
                ("username".to_owned(), "bot".to_owned()),
            ]
            .into(),
        });
        assert_eq!(updated, Ok(ConnectionEvent::Updated));
        assert_eq!(connection.url(), Some("https://crm.example/api"));
        assert_eq!(connection.fields()["tenant"], "acme");

        let _ = connection
            .update(UpdateConnection {
                url: Some(None),
                fields: [("tenant".to_owned(), "".to_owned())].into(),
            })
            .unwrap();
        assert_eq!(connection.url(), None);
        assert_eq!(connection.fields().len(), 1);
    }

    #[test]
    fn only_a_secret_its_auth_style_takes_is_set_and_clearing_an_unset_one_changes_nothing() {
        let mut connection = declared();
        assert_eq!(
            connection.set_secret("token", secret()),
            Err(ConnectionError::UnknownSecret("token".into()))
        );
        assert_eq!(connection.clear_secret("password"), Ok(None));
        assert_eq!(
            connection.set_secret("password", secret()),
            Ok(ConnectionEvent::SecretSet {
                name: "password".into()
            })
        );
        assert_eq!(
            connection.clear_secret("password"),
            Ok(Some(ConnectionEvent::SecretCleared {
                name: "password".into()
            }))
        );
    }

    #[test]
    fn redeclaring_drops_the_values_the_declaration_no_longer_names() {
        let mut connection = declared();
        let _ = connection.set_secret("password", secret()).unwrap();
        let _ = connection
            .update(UpdateConnection {
                url: None,
                fields: [("tenant".to_owned(), "acme".to_owned())].into(),
            })
            .unwrap();
        assert_eq!(connection.redeclare(crm()), Ok(None));

        let bearer = Declaration::new("crm", "CRM").auth(Auth::Bearer);
        assert_eq!(
            connection.redeclare(bearer),
            Ok(Some(ConnectionEvent::Redeclared))
        );
        assert!(connection.fields().is_empty());
        assert!(connection.secrets().is_empty());
    }

    #[test]
    fn a_retired_connection_is_declared_again_with_its_values() {
        let mut connection = declared();
        let _ = connection.set_secret("password", secret()).unwrap();
        assert_eq!(connection.retire(), Some(ConnectionEvent::Retired));
        assert_eq!(connection.retire(), None);
        assert_eq!(
            connection.redeclare(crm()),
            Ok(Some(ConnectionEvent::Redeclared))
        );
        assert!(connection.declared());
        assert!(connection.secret("password").is_some());
    }
}
