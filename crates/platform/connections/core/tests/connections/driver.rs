//! The only place in these tests that knows the transport (ADR 0004).

use app_authz::{Authorizer, Rows};
use app_db::{Clause, Columns, Worker};
use app_domain::{FixedClock, seed_time};
use app_events::{Event, PayloadOf};
use app_module::Context;
use app_persistence::Version;
use app_test_db::Template;
use app_web::testing::{Call, Page, Refused, TestApi};
use app_web::{Accept, JwtVerifier, json_newtype};
use connections::{
    ConnectionKey, ConnectionsEndpoints, EnvKeyProvider, KeyError, KeyProvider, SealedSecret,
};
use connections_api::{Auth, Clients, Declaration, HealthCheck};
use poem::EndpointExt;
use serde_json::{Value, json};
use sqlx::{PgPool, QueryBuilder};
use std::sync::{Arc, Mutex};

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A rule was broken: the paths of the inputs at fault, if it names any.
    Invalid(Vec<String>),
    NotSignedIn,
    /// Signed in, but the policies don't allow it.
    Forbidden,
    NotFound,
    PreconditionRequired,
    Stale,
    /// No master key is configured.
    NotConfigured,
}

impl Refusal {
    pub fn invalid<const N: usize>(fields: [&str; N]) -> Self {
        Refusal::Invalid(fields.map(String::from).to_vec())
    }
}

impl From<Refused> for Refusal {
    fn from(refused: Refused) -> Self {
        match refused.code.as_str() {
            "validation_error" => Refusal::Invalid(refused.paths),
            "unauthorized" => Refusal::NotSignedIn,
            "forbidden" => Refusal::Forbidden,
            "not_found" => Refusal::NotFound,
            "precondition_required" => Refusal::PreconditionRequired,
            "precondition_failed" => Refusal::Stale,
            "not_configured" => Refusal::NotConfigured,
            other => panic!("unexpected error code {other}"),
        }
    }
}

pub type Outcome<T> = Result<T, Refusal>;

json_newtype!(pub ConnectionJson);

impl ConnectionJson {
    /// The secret `name` as the Connection answers it.
    pub fn secret(&self, name: &str) -> &Value {
        self["secrets"]
            .as_array()
            .and_then(|secrets| secrets.iter().find(|s| s["name"] == name))
            .unwrap_or_else(|| panic!("no secret {name} in {}", self.0))
    }

    pub fn field(&self, name: &str) -> &Value {
        let field = self["fields"]
            .as_array()
            .and_then(|fields| fields.iter().find(|f| f["name"] == name))
            .unwrap_or_else(|| panic!("no field {name} in {}", self.0));
        &field["value"]
    }
}

pub type ConnectionsPage = Page<ConnectionJson>;

pub const CRM: &str = "crm";
pub const BILLING: &str = "billing";
pub const LEDGER: &str = "ledger";
pub const ERP: &str = "erp";
pub const SEARCH: &str = "search";
pub const FEED: &str = "feed";

/// One Connection of each auth style, as modules would declare them.
pub fn declarations() -> Vec<Declaration> {
    vec![
        crm(),
        Declaration::new(BILLING, "Billing").auth(Auth::Bearer),
        Declaration::new(LEDGER, "Ledger").auth(Auth::Basic),
        Declaration::new(ERP, "ERP").auth(Auth::OAuth2ClientCredentials {
            scopes: vec!["orders.read".into()],
        }),
        Declaration::new(SEARCH, "Search").auth(Auth::ApiKeyQuery { name: "key".into() }),
        Declaration::new(FEED, "Public feed"),
    ]
}

pub fn crm() -> Declaration {
    Declaration::new(CRM, "CRM")
        .auth(Auth::ApiKeyHeader {
            name: "X-Api-Key".into(),
        })
        .field("tenant", "Tenant")
        .health_check(HealthCheck::get("/health"))
}

/// The master key the tests' server seals with.
pub const MASTER_KEY: [u8; 32] = [42; 32];

pub const ADMIN: &str = "admin";
pub const USER: &str = "user";
/// Whom every test signs in as.
pub const SUBJECT: &str = "a-user";

pub struct Connections {
    api: TestApi,
    ctx: Context,
    worker: Worker,
    seeded_through: i64,
    verifier: JwtVerifier,
    caller: Mutex<Option<String>>,
    clients: Arc<dyn Clients>,
}

impl Connections {
    /// Sealing with [`MASTER_KEY`], signed in as an `admin`.
    pub async fn seeded(pool: PgPool) -> Self {
        let keys: Arc<dyn KeyProvider> = Arc::new(EnvKeyProvider::new(MASTER_KEY));
        Self::boot(pool, Some(keys)).await
    }

    /// As a deployment with no `CONNECTIONS_MASTER_KEY`.
    pub async fn without_a_master_key(pool: PgPool) -> Self {
        Self::boot(pool, None).await
    }

    async fn boot(pool: PgPool, keys: Option<Arc<dyn KeyProvider>>) -> Self {
        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let policies = connections::policies()
            .build()
            .expect("connections' policies load");
        let authorizer =
            Authorizer::from_policies(&[policies]).expect("connections' policies compile");
        let ctx = Context::new(pool, clock).with_authorizer(Arc::new(authorizer));
        let verifier = JwtVerifier::new("test-secret");
        let api = TestApi::new(ConnectionsEndpoints, ctx.idempotency(), |app| {
            app.accept(connections::provider(&ctx, keys.clone()))
                .data(verifier.clone())
                .data(ctx.authorizer())
        });
        let worker = ctx.worker(Default::default());
        let seeded_through = worker.outbox_end().await.expect("read the outbox");
        let clients = connections::clients_with(&ctx, keys);
        let connections = Self {
            clients,
            api,
            ctx,
            worker,
            seeded_through,
            verifier,
            caller: Mutex::new(None),
        };
        connections.sign_in_with_roles(&[ADMIN]);
        connections
    }

    /// What another module calls out through.
    pub fn clients(&self) -> &dyn Clients {
        self.clients.as_ref()
    }

    pub async fn test(&self, key: &str) -> Outcome<ConnectionJson> {
        self.signed(self.api.post(format!("/api/connections/{key}/test")))
            .send()
            .await
    }

    /// Gives `key` its URL, `fields` and `secrets`, as an admin would.
    pub async fn configure(
        &self,
        key: &str,
        url: &str,
        fields: Value,
        secrets: &[(&str, &str)],
    ) -> ConnectionJson {
        let version: i64 = self.get(key).await.expect("declared")["version"]
            .as_str()
            .and_then(|v| v.parse().ok())
            .expect("a version");
        let mut connection = self
            .update(key, version, json!({ "url": url, "fields": fields }))
            .await
            .expect("the URL and fields are set");
        for (name, value) in secrets {
            let version = connection["version"].as_str().unwrap().parse().unwrap();
            connection = self
                .set_secret(key, name, version, value)
                .await
                .expect("the secret is set");
        }
        connection
    }

    pub fn authorizer(&self) -> Arc<Authorizer> {
        self.ctx.authorizer()
    }

    /// The keys of the stored Connections `rows` holds, queried as the repository's list
    /// scopes it, through `C`'s columns.
    pub async fn keys_where<C: Columns>(&self, rows: Rows) -> Vec<String> {
        let mut sql = QueryBuilder::new("SELECT key FROM connections.connection");
        app_db::push_scope::<C>(&mut sql, Clause::Where, &rows);
        sql.build_query_scalar()
            .fetch_all(&self.ctx.pool())
            .await
            .expect("query the connections")
    }

    /// Later requests present a real token of [`SUBJECT`] holding `user` and `roles`,
    /// assigned in the rule set in force as `access` would.
    pub fn sign_in_with_roles(&self, roles: &[&str]) {
        self.ctx.authorizer().assign_in_memory(SUBJECT, roles);
        let token = self.verifier.issue(SUBJECT, 60).expect("issue token");
        *self.caller.lock().unwrap() = Some(token);
    }

    /// Signed in holding only `user`, as every signed-in user does.
    pub fn sign_in_as_a_user(&self) {
        self.sign_in_with_roles(&[]);
    }

    pub fn sign_out(&self) {
        *self.caller.lock().unwrap() = None;
    }

    fn signed<'c>(&self, call: Call<'c>) -> Call<'c> {
        match self.caller.lock().unwrap().as_ref() {
            Some(token) => call.bearer(token),
            None => call,
        }
    }

    /// Every `E` connections has announced, oldest first, with the version of the Connection
    /// each announces, as the outbox holds it.
    pub async fn announced<E: Event>(&self) -> Vec<(Version, PayloadOf<E>)> {
        self.worker
            .published_after::<E>(self.seeded_through)
            .await
            .expect("read the outbox")
            .into_iter()
            .map(|event| (Version::from_i64(event.entity_version), event.data))
            .collect()
    }

    /// Everything announced since boot, as stored in the outbox, payloads and all.
    pub async fn outbox(&self) -> String {
        let payloads: Vec<Value> = sqlx::query_scalar(
            "SELECT to_jsonb(o) FROM messaging.outbox o WHERE seq > $1 ORDER BY seq",
        )
        .bind(self.seeded_through)
        .fetch_all(&self.ctx.pool())
        .await
        .expect("read the outbox");
        Value::from(payloads).to_string()
    }

    /// As `migrate` records what the build's modules declare. Returns how many changed.
    pub async fn declare(&self, declarations: Vec<Declaration>) -> usize {
        connections::declare(&self.ctx, declarations)
            .await
            .expect("the declarations are recorded")
    }

    pub async fn declare_refused(&self, declarations: Vec<Declaration>) -> Outcome<usize> {
        connections::declare(&self.ctx, declarations)
            .await
            .map_err(|err| match err {
                app_error::AppError::Validation { .. } => Refusal::Invalid(Vec::new()),
                other => panic!("unexpected refusal {other}"),
            })
    }

    /// The secret `name` of `key` as stored, sealed.
    pub async fn stored_secret(&self, key: &str, name: &str) -> SealedSecret {
        let (ciphertext, wrapped_key, key_version) = sqlx::query_as(
            "SELECT ciphertext, wrapped_key, key_version FROM connections.secret \
             WHERE connection_key = $1 AND name = $2",
        )
        .bind(key)
        .bind(name)
        .fetch_one(&self.ctx.pool())
        .await
        .expect("the secret is stored");
        SealedSecret {
            ciphertext,
            wrapped_key,
            key_version,
        }
    }

    /// Opens the secret `name` of `key` as stored, with `master_key`.
    pub async fn unseal(
        &self,
        key: &str,
        name: &str,
        master_key: [u8; 32],
    ) -> Result<String, KeyError> {
        let sealed = self.stored_secret(key, name).await;
        let key = ConnectionKey::parse(key).expect("a key");
        connections::open(
            &EnvKeyProvider::new(master_key),
            &connections::sealed_for(&key, name),
            &sealed,
        )
        .await
    }

    pub async fn list(&self) -> Outcome<ConnectionsPage> {
        self.signed(self.api.get("/api/connections")).send().await
    }

    pub async fn keys(&self) -> Vec<String> {
        self.list()
            .await
            .expect("listing connections")
            .items
            .iter()
            .map(|c| c["key"].as_str().unwrap_or_default().to_owned())
            .collect()
    }

    pub async fn get(&self, key: &str) -> Outcome<ConnectionJson> {
        self.signed(self.api.get(format!("/api/connections/{key}")))
            .send()
            .await
    }

    /// `changes` as the API takes them: `url`, `fields`.
    pub async fn update(&self, key: &str, version: i64, changes: Value) -> Outcome<ConnectionJson> {
        self.signed(self.api.patch(format!("/api/connections/{key}")))
            .if_match(version)
            .json(&changes)
            .send()
            .await
    }

    pub async fn set_url(&self, key: &str, version: i64, url: &str) -> Outcome<ConnectionJson> {
        self.update(key, version, json!({ "url": url })).await
    }

    pub async fn set_secret(
        &self,
        key: &str,
        name: &str,
        version: i64,
        value: &str,
    ) -> Outcome<ConnectionJson> {
        self.signed(
            self.api
                .put(format!("/api/connections/{key}/secrets/{name}")),
        )
        .if_match(version)
        .json(&json!({ "value": value }))
        .send()
        .await
    }

    pub async fn set_secret_without_precondition(
        &self,
        key: &str,
        name: &str,
        value: &str,
    ) -> Outcome<ConnectionJson> {
        self.signed(
            self.api
                .put(format!("/api/connections/{key}/secrets/{name}")),
        )
        .json(&json!({ "value": value }))
        .send()
        .await
    }

    pub async fn clear_secret(
        &self,
        key: &str,
        name: &str,
        version: i64,
    ) -> Outcome<ConnectionJson> {
        self.signed(
            self.api
                .delete(format!("/api/connections/{key}/secrets/{name}")),
        )
        .if_match(version)
        .send()
        .await
    }
}

/// Every test's database starts as a copy of this: migrated, with [`declarations`] recorded
/// as `migrate` records them, and their events relayed.
pub const TEMPLATE: Template = Template::new("connections", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &[connections::migrations()])
            .await
            .expect("migrate connections");
        let ctx = Context::new(pool.clone(), Arc::new(FixedClock::new(seed_time(0))));
        connections::declare(&ctx, declarations())
            .await
            .expect("record the declarations");
        app_db::Worker::new(
            pool.clone(),
            Arc::new(app_domain::SystemClock),
            Default::default(),
        )
        .relay()
        .await
        .expect("relay the declarations' events");
    })
});
