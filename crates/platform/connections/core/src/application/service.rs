use super::{
    ConnectionRepository, ConnectionSortKey, EventPublisher, Expected, KeyProvider, ListQuery,
    Listed, Transactions, Version, Versioned, events, seal,
};
use crate::domain::{
    Connection, ConnectionAction, ConnectionError, ConnectionEvent, ConnectionKey, Secret,
    UpdateConnection, secret_value,
};
use app_authz::{Authorizer, Plan, Principal};
use app_domain::Clock;
use app_error::{AppError, AppResult};
use connections_api::Declaration;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone)]
pub struct ConnectionService {
    repo: Arc<dyn ConnectionRepository>,
    /// `None` when no master key is configured: secrets can't be set.
    keys: Option<Arc<dyn KeyProvider>>,
    publisher: Arc<dyn EventPublisher>,
    transactions: Arc<dyn Transactions>,
    clock: Arc<dyn Clock>,
    authorizer: Arc<Authorizer>,
}

/// What a secret is sealed with besides its key: where it belongs, so it opens nowhere else.
pub fn sealed_for(key: &ConnectionKey, name: &str) -> String {
    format!("connections/{key}/{name}")
}

impl ConnectionService {
    pub fn new(
        repo: Arc<dyn ConnectionRepository>,
        keys: Option<Arc<dyn KeyProvider>>,
        publisher: Arc<dyn EventPublisher>,
        transactions: Arc<dyn Transactions>,
        clock: Arc<dyn Clock>,
        authorizer: Arc<Authorizer>,
    ) -> Self {
        Self {
            repo,
            keys,
            publisher,
            transactions,
            clock,
            authorizer,
        }
    }

    fn authorize(&self, principal: &Principal, action: ConnectionAction) -> AppResult<Plan> {
        self.authorizer.authorize::<Connection>(principal, action)
    }

    pub async fn list(
        &self,
        principal: &Principal,
        query: ListQuery<ConnectionSortKey>,
    ) -> AppResult<Listed<Versioned<Connection>>> {
        let rows = self
            .authorizer
            .rows::<Connection>(principal, ConnectionAction::List)?;
        self.repo.list(&query, &rows).await
    }

    pub async fn get(&self, principal: &Principal, key: &str) -> AppResult<Versioned<Connection>> {
        let plan = self.authorize(principal, ConnectionAction::View)?;
        self.load_for(principal, &plan, key).await
    }

    /// `expected` is the version the caller last read (from `If-Match`).
    pub async fn update(
        &self,
        principal: &Principal,
        key: &str,
        cmd: UpdateConnection,
        expected: Version,
    ) -> AppResult<Versioned<Connection>> {
        let plan = self.authorize(principal, ConnectionAction::Edit)?;
        let mut connection = self.load_for(principal, &plan, key).await?.entity;
        let event = connection.update(cmd)?;
        self.save(connection, Expected::Version(expected), event)
            .await
    }

    /// Seals `value` before it is stored; refused `not_configured` with no master key.
    pub async fn set_secret(
        &self,
        principal: &Principal,
        key: &str,
        name: &str,
        value: &str,
        expected: Version,
    ) -> AppResult<Versioned<Connection>> {
        let plan = self.authorize(principal, ConnectionAction::SetSecret)?;
        let keys = self.keys.as_ref().ok_or_else(|| {
            AppError::not_configured(
                "secrets can't be set: no master key is configured (CONNECTIONS_MASTER_KEY)",
            )
        })?;
        let value = secret_value(value)?;
        let mut connection = self.load_for(principal, &plan, key).await?.entity;
        connection.takes_secret(name)?;
        let sealed = seal(keys.as_ref(), &sealed_for(connection.key(), name), value)
            .await
            .map_err(AppError::internal)?;
        let secret = Secret {
            sealed,
            set_at: self.clock.now(),
            set_by: principal.id().unwrap_or("system").to_owned(),
        };
        let event = connection.set_secret(name, secret)?;
        self.save(connection, Expected::Version(expected), event)
            .await
    }

    /// Clearing a secret that isn't set changes nothing.
    pub async fn clear_secret(
        &self,
        principal: &Principal,
        key: &str,
        name: &str,
        expected: Version,
    ) -> AppResult<Versioned<Connection>> {
        let plan = self.authorize(principal, ConnectionAction::ClearSecret)?;
        let loaded = self.load_for(principal, &plan, key).await?;
        let mut connection = loaded.entity;
        match connection.clear_secret(name)? {
            Some(event) => {
                self.save(connection, Expected::Version(expected), event)
                    .await
            }
            None if loaded.version == expected => Ok(Versioned {
                entity: connection,
                version: loaded.version,
            }),
            None => Err(stale()),
        }
    }

    /// What `migrate` runs, as `System`: records every Connection the modules declare, and
    /// retires those none does any more, in one unit of work. Returns how many it changed.
    pub async fn declare(&self, declarations: Vec<Declaration>) -> AppResult<usize> {
        let mut keys = BTreeSet::new();
        for declaration in &declarations {
            if !keys.insert(declaration.key.clone()) {
                return Err(ConnectionError::DeclaredTwice(declaration.key.clone()).into());
            }
        }
        let mut stored: BTreeMap<String, Versioned<Connection>> = self
            .repo
            .all()
            .await?
            .into_iter()
            .map(|c| (c.entity.key().to_string(), c))
            .collect();

        let mut changes = Vec::new();
        for declaration in declarations {
            match stored.remove(&declaration.key) {
                None => {
                    let (connection, event) = Connection::declare(declaration)?;
                    changes.push((connection, Expected::New, event));
                }
                Some(Versioned {
                    entity: mut connection,
                    version,
                }) => {
                    if let Some(event) = connection.redeclare(declaration)? {
                        changes.push((connection, Expected::Version(version), event));
                    }
                }
            }
        }
        for Versioned {
            entity: mut connection,
            version,
        } in stored.into_values()
        {
            if let Some(event) = connection.retire() {
                changes.push((connection, Expected::Version(version), event));
            }
        }

        let changed = changes.len();
        let mut tx = self.transactions.begin().await?;
        for (connection, expected, event) in changes {
            let version = self.repo.save(&mut tx, &connection, expected).await?;
            self.publisher
                .publish(&mut tx, events::pending(&connection, version, event))
                .await?;
        }
        tx.commit().await?;
        Ok(changed)
    }

    async fn save(
        &self,
        connection: Connection,
        expected: Expected,
        event: ConnectionEvent,
    ) -> AppResult<Versioned<Connection>> {
        let mut tx = self.transactions.begin().await?;
        let version = self.repo.save(&mut tx, &connection, expected).await?;
        self.publisher
            .publish(&mut tx, events::pending(&connection, version, event))
            .await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: connection,
            version,
        })
    }

    /// The declared Connection `key`, if `plan` allows it: one no module declares any more is
    /// not found, as is one the principal may not see.
    async fn load_for(
        &self,
        principal: &Principal,
        plan: &Plan,
        key: &str,
    ) -> AppResult<Versioned<Connection>> {
        let key = ConnectionKey::parse(key)?;
        let connection = self
            .repo
            .get(&key)
            .await?
            .filter(|c| c.entity.declared())
            .ok_or_else(|| AppError::not_found("connection"))?;
        self.authorizer.check(principal, plan, &connection.entity)?;
        Ok(connection)
    }
}

fn stale() -> AppError {
    AppError::precondition_failed("the connection was modified by someone else")
}
