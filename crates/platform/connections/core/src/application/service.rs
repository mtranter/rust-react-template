use super::calling::{ConnectionClient, Tokens, Transport, status_text};
use super::{
    ConnectionRepository, ConnectionSortKey, EventPublisher, Expected, KeyProvider, ListQuery,
    Listed, Transactions, Version, Versioned, events, seal,
};
use crate::domain::{
    Connection, ConnectionAction, ConnectionError, ConnectionEvent, ConnectionKey, Health, Outcome,
    Secret, Status, UpdateConnection, secret_value,
};
use app_authz::{Authorizer, Plan, Principal};
use app_domain::Clock;
use app_error::{AppError, AppResult};
use connections_api::{CallError, Client, Clients, Declaration, Request};
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
    transport: Arc<dyn Transport>,
    tokens: Arc<Tokens>,
}

/// A Connection as an admin reads it: with what its last test found.
pub struct Reported {
    pub connection: Versioned<Connection>,
    pub status: Status,
    pub tested_at: Option<chrono::DateTime<chrono::Utc>>,
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
        transport: Arc<dyn Transport>,
    ) -> Self {
        Self {
            repo,
            keys,
            publisher,
            transactions,
            clock,
            authorizer,
            transport,
            tokens: Arc::default(),
        }
    }

    /// Each with its status: a test counts only for the version it tested.
    pub async fn reported(
        &self,
        connections: Vec<Versioned<Connection>>,
    ) -> AppResult<Vec<Reported>> {
        let keys: Vec<String> = connections
            .iter()
            .map(|c| c.entity.key().to_string())
            .collect();
        let mut health = self.repo.health(&keys).await?;
        Ok(connections
            .into_iter()
            .map(|connection| {
                let tested = health
                    .remove(connection.entity.key().as_str())
                    .filter(|(version, _)| *version == connection.version)
                    .map(|(_, health)| health);
                Reported {
                    status: Status::of(connection.entity.configured(), tested.as_ref()),
                    tested_at: tested.map(|h| h.tested_at),
                    connection,
                }
            })
            .collect())
    }

    pub async fn report(&self, connection: Versioned<Connection>) -> AppResult<Reported> {
        Ok(self
            .reported(vec![connection])
            .await?
            .pop()
            .expect("one in, one out"))
    }

    /// Sends the Connection's declared health check and records what it found: `ok` on a 2xx,
    /// else `failing` with why, in plain language. Refused `not_configured` until it is filled
    /// in. Changes neither the Connection nor its version.
    pub async fn test(&self, principal: &Principal, key: &str) -> AppResult<Reported> {
        let plan = self.authorize(principal, ConnectionAction::Test)?;
        let loaded = self.load_for(principal, &plan, key).await?;
        let check = loaded
            .entity
            .declaration()
            .health_check
            .clone()
            .ok_or_else(|| {
                AppError::validation(format!(
                    "the Connection `{key}` declares no health check to test it with"
                ))
            })?;
        let client = self.open(&loaded).await.map_err(AppError::from)?;
        let request = Request::new(check.method.into(), check.path.clone());
        let what = format!("{} {}", request.method.as_str(), check.path);
        let outcome = match client.send(request).await {
            Ok(answer) if answer.is_success() => Outcome::Ok,
            Ok(answer) => Outcome::Failing(match answer.status {
                401 | 403 => format!(
                    "{what} answered {}: check the credentials",
                    status_text(answer.status)
                ),
                404 => format!(
                    "{what} answered {}: check the URL",
                    status_text(answer.status)
                ),
                status => format!("{what} answered {}", status_text(status)),
            }),
            Err(err @ CallError::NotConfigured(_)) => return Err(err.into()),
            Err(err) => Outcome::Failing(format!("{what} failed: {err}")),
        };
        let health = Health {
            outcome,
            tested_at: self.clock.now(),
        };
        self.repo
            .record_health(loaded.entity.key(), loaded.version, &health)
            .await?;
        self.report(loaded).await
    }

    async fn open(
        &self,
        connection: &Versioned<Connection>,
    ) -> Result<ConnectionClient, CallError> {
        ConnectionClient::open(
            &connection.entity,
            connection.version.as_i64(),
            self.keys.as_deref(),
            self.transport.clone(),
            self.tokens.clone(),
            self.clock.clone(),
        )
        .await
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

/// As `System`: the module that declared the Connection calls through it, whoever asked it to.
#[async_trait::async_trait]
impl Clients for ConnectionService {
    async fn client(&self, key: &str) -> Result<Arc<dyn Client>, CallError> {
        let undeclared = || CallError::Undeclared(key.to_owned());
        let parsed = ConnectionKey::parse(key).map_err(|_| undeclared())?;
        let connection = self
            .repo
            .get(&parsed)
            .await
            .map_err(|_| CallError::Unreachable("the Connection couldn't be read".into()))?
            .filter(|c| c.entity.declared())
            .ok_or_else(undeclared)?;
        Ok(Arc::new(self.open(&connection).await?))
    }
}

fn stale() -> AppError {
    AppError::precondition_failed("the connection was modified by someone else")
}
