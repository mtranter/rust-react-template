//! The only place in the journeys that knows the transport (ADR 0029). Unlike a
//! module's driver it stubs nothing: it boots the reference modules' composed
//! app as `server` does, with the composed worker and real JWT auth.

use app_authz::Principal;
use app_db::{DeadLetter, Worker};
use app_domain::{Clock, FixedClock, seed_time};
use app_events::Event;
use app_module::Context;
use app_test_db::Template;
use app_web::mcp::Discovery;
use app_web::testing::{Call, McpSession, Page, Refused, TestApi};
use app_web::{JwtVerifier, json_newtype};
use chrono::{DateTime, TimeDelta, Utc};
use reference_journeys::Reference;
use serde_json::json;
use server::Composition;
use sqlx::PgPool;
use std::fmt::Display;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A rule was broken: the paths of the inputs at fault, if it names any.
    Invalid(Vec<String>),
    /// The request couldn't be parsed.
    Malformed,
    NotSignedIn,
    /// Signed in, but the policies don't allow it.
    Forbidden,
    NotFound,
    Conflict,
    PreconditionRequired,
    Stale,
    /// The idempotency key was sent before with another request.
    KeyReused,
}

impl From<Refused> for Refusal {
    fn from(refused: Refused) -> Self {
        match refused.code.as_str() {
            "validation_error" => Refusal::Invalid(refused.paths),
            "malformed_request" => Refusal::Malformed,
            "unauthorized" => Refusal::NotSignedIn,
            "forbidden" => Refusal::Forbidden,
            "not_found" => Refusal::NotFound,
            "conflict" => Refusal::Conflict,
            "precondition_required" => Refusal::PreconditionRequired,
            "precondition_failed" => Refusal::Stale,
            "idempotency_key_reused" => Refusal::KeyReused,
            other => panic!("unexpected error code {other}"),
        }
    }
}

pub type Outcome<T> = Result<T, Refusal>;

json_newtype!(pub UserJson);

json_newtype!(pub NoteJson);

json_newtype!(pub InvitationJson);

impl InvitationJson {
    pub fn subject(&self) -> &str {
        self["subject"].as_str().unwrap_or_default()
    }
}

impl NoteJson {
    /// As the note reads once archived at `at`, by one more save.
    pub fn archived(&self, at: DateTime<Utc>) -> Self {
        let mut json = self.0.clone();
        json["archivedAt"] = at.to_rfc3339().into();
        json["version"] = (self.version() + 1).to_string().into();
        Self(json)
    }
}

json_newtype!(pub ConnectionJson);

impl ConnectionJson {
    pub fn key(&self) -> &str {
        self["key"].as_str().unwrap_or_default()
    }
}

json_newtype!(pub GrantJson);

json_newtype!(pub AssignmentJson);

json_newtype!(pub CatalogJson);

json_newtype!(pub NoteSummaryJson);

impl NoteSummaryJson {
    /// How a search lists `note`, by `author` as they now are.
    pub fn of(note: &NoteJson, author: &UserJson) -> Self {
        Self(json!({
            "id": note["id"],
            "title": note["title"],
            "authorId": note["authorId"],
            "createdAt": note["createdAt"],
            "authorName": author["displayName"],
        }))
    }
}

pub type Found = Page<NoteSummaryJson>;

/// Whom the journeys' own checks read as: an `admin` (`TEMPLATE` assigns it),
/// who sees every note, and is none of the seeded users.
const READER: &str = "00000000-0000-0000-0000-000000000000";
const ADMIN: &str = "admin";

/// Assigns `role` to `subject` through `access`' Module API, as a seed does.
async fn assign(ctx: &Context, subject: &str, role: &str) {
    access::assignments(ctx)
        .ensure_assigned(&Principal::System, subject, role)
        .await
        .expect("assign the role");
}

pub struct App {
    api: TestApi,
    verifier: JwtVerifier,
    worker: Worker,
    clock: Arc<FixedClock>,
    seeded_through: i64,
    pool: PgPool,
    rules: access::Rules,
}

impl App {
    /// Records `declaration` as `migrate` does when a module declares it
    /// (`Module::declaring`): no reference module declares a Connection yet.
    pub async fn declares(&self, declaration: connections_api::Declaration) {
        let ctx = Context::new(self.pool.clone(), self.clock.clone());
        connections::declare(&ctx, vec![declaration])
            .await
            .expect("the declaration is recorded");
    }

    /// Migrates, seeds and registers topics, subscriptions and schedules for every
    /// module, as `migrate` and `seed` do, then builds the app and worker as `server` does.
    pub async fn seeded(pool: PgPool) -> Self {
        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let app = Self::instance(pool, clock).await;
        let ctx = Context::new(app.pool.clone(), app.clock.clone());
        server::register_topics::<Reference>(&ctx)
            .await
            .expect("register every module's topics");
        server::register_subscriptions::<Reference>(&ctx)
            .await
            .expect("register every module's subscriptions");
        server::register_schedules::<Reference>(&ctx)
            .await
            .expect("register every module's schedules");
        app
    }

    /// One server instance over the database, serving once it has read the
    /// stored grants, as `server` does, inviting through `fake-oidc`'s stand-in
    /// for Cognito.
    async fn instance(pool: PgPool, clock: Arc<FixedClock>) -> Self {
        user_pool();
        master_key();
        let authorizer = server::authorizer::<Reference>().expect("every module's policies load");
        let ctx = Context::new(pool.clone(), clock.clone()).with_authorizer(Arc::new(authorizer));
        let rules = server::rules::<Reference>(&ctx).expect("every module's policies load");
        rules.refresh().await.expect("the grants are read");
        let worker = server::worker::<Reference>(&ctx);
        let seeded_through = worker.outbox_end().await.expect("read the outbox");
        let verifier = JwtVerifier::new("test-secret");
        let api = TestApi::over(server::build_app::<Reference>(
            &ctx,
            verifier.clone(),
            Discovery::default(),
            None,
            "http://localhost",
        ));
        Self {
            api,
            verifier,
            worker,
            clock,
            seeded_through,
            pool,
            rules,
        }
    }

    /// A second server instance over the same database, as behind a load balancer.
    pub async fn another_instance(&self) -> Self {
        Self::instance(self.pool.clone(), self.clock.clone()).await
    }

    /// The instance's next poll of the grants' revision, as it makes every
    /// `access::POLL_EVERY`: what bounds how long a change of grants or
    /// assignments takes to apply.
    pub async fn polls_the_grants(&self) {
        self.rules.refresh().await.expect("the grants are read");
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// Delivers the events announced so far, as the composed worker would.
    pub async fn run_pending_events(&self) {
        self.worker
            .run_until_idle()
            .await
            .expect("the worker reaches the database");
    }

    pub fn later(&self, by: TimeDelta) {
        self.clock.advance(by);
    }

    /// Compacts the outbox, as the composed worker does on its schedule.
    pub async fn compact(&self) {
        self.worker
            .compact()
            .await
            .expect("the worker reaches the database");
    }

    /// The root of every `E` the outbox still holds but the seeds', by id,
    /// oldest first.
    pub async fn keys_announcing<E: Event>(&self) -> Vec<String> {
        self.worker
            .published_after::<E>(self.seeded_through)
            .await
            .expect("read the outbox")
            .into_iter()
            .map(|event| event.partition_key)
            .collect()
    }

    /// Background work that ran out of attempts.
    pub async fn given_up_on(&self) -> Vec<DeadLetter> {
        self.worker.dead_letters().await.expect("read dead letters")
    }

    pub async fn note(&self, id: impl Display) -> Outcome<NoteJson> {
        self.api
            .get(format!("/api/notes/{id}"))
            .bearer(self.token_of(READER))
            .send()
            .await
    }

    /// Once the worker has delivered what was announced: note-search answers
    /// from its own copy, kept from the events.
    pub async fn search_notes(&self, text: &str) -> Found {
        self.run_pending_events().await;
        let found: Outcome<Found> = self
            .api
            .get("/api/note-search")
            .bearer(self.token_of(READER))
            .query("q", text)
            .send()
            .await;
        found.expect("searching never fails")
    }

    /// Signed in with a real token, which the server verifies, holding `user`
    /// and whatever `access` assigns them. The seeds assign Alice `admin` (she
    /// is the dev sign-in), so a journey that needs a plain user uses Bob.
    pub fn as_user(&self, user: impl Display) -> Session<'_> {
        Session {
            app: self,
            token: self.token_of(user),
        }
    }

    /// Signed in as an `admin`, who manages who may do what: assigned `admin`
    /// through `access`, as a seed does, from this instance's next poll.
    pub async fn as_admin(&self, user: impl Display) -> Admin<'_> {
        let user = user.to_string();
        let ctx = Context::new(self.pool.clone(), self.clock.clone());
        assign(&ctx, &user, ADMIN).await;
        self.polls_the_grants().await;
        Admin {
            app: self,
            token: self.token_of(user),
        }
    }

    fn token_of(&self, user: impl Display) -> String {
        self.verifier
            .issue(user.to_string(), 60)
            .expect("issue token")
    }

    /// An AI agent acting as `user` over MCP, with a real token.
    pub async fn as_agent(&self, user: impl Display) -> Agent<'_> {
        let token = self.token_of(&user);
        let session = self
            .api
            .mcp_session::<Refusal>(Some(&token))
            .await
            .expect("an agent with a token opens a session");
        Agent { session }
    }

    /// An agent opening an MCP session without a token.
    pub async fn agent_signed_out(&self) -> Outcome<()> {
        self.api.mcp_session::<Refusal>(None).await.map(|_| ())
    }
}

/// `fake-oidc`'s stand-in for the Cognito user pool API, served once for the
/// test binary, which `access` invites through: `COGNITO_*` are read as each
/// instance is built, and every instance of every journey sees the same.
fn user_pool() {
    static SERVED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    SERVED.get_or_init(|| {
        let endpoint = app_fake_oidc::UserPool::serve();
        // SAFETY: set once, before any instance reads them, and never changed.
        unsafe {
            std::env::set_var("COGNITO_USER_POOL_ID", "ap-southeast-2_journeys");
            std::env::set_var("COGNITO_ENDPOINT", endpoint);
            std::env::set_var("AWS_ACCESS_KEY_ID", "journeys");
            std::env::set_var("AWS_SECRET_ACCESS_KEY", "journeys");
        }
    });
}

/// What `connections` seals secrets with, as a deployment's `CONNECTIONS_MASTER_KEY`, read as
/// each instance is built.
fn master_key() {
    static SET: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    SET.get_or_init(|| {
        use base64::Engine;
        let key = base64::engine::general_purpose::STANDARD.encode([42_u8; 32]);
        // SAFETY: set once, before any instance reads it, and never changed.
        unsafe { std::env::set_var("CONNECTIONS_MASTER_KEY", key) };
    });
}

/// An AI agent's MCP session: each verb a tool call.
pub struct Agent<'a> {
    session: McpSession<'a>,
}

impl Agent<'_> {
    /// The tools the agent is offered, by name.
    pub async fn tools(&self) -> Vec<String> {
        self.session
            .tool_names::<Refusal>()
            .await
            .expect("the agent is listed its tools")
    }

    pub async fn me(&self) -> Outcome<UserJson> {
        self.session.call("get_me", json!({})).await
    }

    pub async fn note(&self, id: impl Display) -> Outcome<NoteJson> {
        self.session
            .call("get_note", json!({ "id": id.to_string() }))
            .await
    }

    /// As the agent's user, like [`Session::write_note`]; retried as one
    /// change when `key` is given.
    pub async fn write_note(
        &self,
        title: &str,
        body: &str,
        key: Option<&str>,
    ) -> Outcome<NoteJson> {
        let mut arguments = json!({
            "body": { "title": title, "body": body },
        });
        if let Some(key) = key {
            arguments["idempotency_key"] = key.into();
        }
        self.session.call("create_note", arguments).await
    }

    pub async fn edit_note(
        &self,
        note: &NoteJson,
        expected_version: i64,
        title: &str,
        body: &str,
    ) -> Outcome<NoteJson> {
        self.session
            .call(
                "edit_note",
                json!({
                    "id": note.id(),
                    "expected_version": expected_version.to_string(),
                    "body": { "title": title, "body": body },
                }),
            )
            .await
    }
}

/// An admin, granting and revoking over the API.
pub struct Admin<'a> {
    app: &'a App,
    token: String,
}

impl Admin<'_> {
    /// `scope` is `any` or one of the resource's scopes.
    pub async fn grant(
        &self,
        role: &str,
        resource: &str,
        action: &str,
        scope: &str,
    ) -> Outcome<GrantJson> {
        self.app
            .api
            .post("/api/access/grants")
            .bearer(&self.token)
            .json(&json!({ "role": role, "resource": resource, "action": action, "scope": scope }))
            .send()
            .await
    }

    /// A role grants can then name.
    pub async fn create_role(&self, name: &str) -> Outcome<serde_json::Value> {
        self.app
            .api
            .post("/api/access/roles")
            .bearer(&self.token)
            .json(&json!({ "name": name }))
            .send()
            .await
    }

    /// `role` to `subject`, the token's `sub`.
    pub async fn assign(&self, subject: impl Display, role: &str) -> Outcome<AssignmentJson> {
        self.app
            .api
            .post("/api/access/assignments")
            .bearer(&self.token)
            .json(&json!({ "subject": subject.to_string(), "role": role }))
            .send()
            .await
    }

    pub async fn unassign(&self, assignment: &AssignmentJson) -> Outcome<()> {
        let field = |name: &str| assignment[name].as_str().unwrap_or_default().to_owned();
        self.app
            .api
            .delete("/api/access/assignments")
            .bearer(&self.token)
            .query("subject", field("subject"))
            .query("role", field("role"))
            .if_match(assignment.version())
            .send_empty()
            .await
    }

    pub async fn revoke(&self, grant: &GrantJson) -> Outcome<()> {
        self.app
            .api
            .delete(format!("/api/access/grants/{}", grant.id()))
            .bearer(&self.token)
            .if_match(grant.version())
            .send_empty()
            .await
    }

    /// The grants, as many as a page holds: more than the journeys make.
    pub async fn grants(&self) -> Outcome<Page<GrantJson>> {
        self.app
            .api
            .get("/api/access/grants")
            .bearer(&self.token)
            .query("limit", 100)
            .send()
            .await
    }

    pub async fn catalog(&self) -> Outcome<CatalogJson> {
        self.app
            .api
            .get("/api/access/catalog")
            .bearer(&self.token)
            .send()
            .await
    }

    /// Invites `email`, who holds `roles` from then on.
    pub async fn invite(&self, email: &str, roles: &[&str]) -> Outcome<InvitationJson> {
        self.app
            .api
            .post("/api/access/invitations")
            .bearer(&self.token)
            .json(&json!({ "email": email, "name": "", "roles": roles }))
            .send()
            .await
    }

    pub async fn invitations(&self) -> Outcome<Page<InvitationJson>> {
        self.app
            .api
            .get("/api/access/invitations")
            .bearer(&self.token)
            .query("limit", 100)
            .send()
            .await
    }

    /// Whom roles can be assigned to, as many as a page holds: more than the journeys sign in.
    pub async fn subjects(&self) -> Outcome<Page<serde_json::Value>> {
        self.app
            .api
            .get("/api/access/subjects")
            .bearer(&self.token)
            .query("limit", 100)
            .send()
            .await
    }
}

pub struct Session<'a> {
    app: &'a App,
    token: String,
}

impl Session<'_> {
    pub async fn connection(&self, key: &str) -> Outcome<ConnectionJson> {
        self.signed(self.app.api.get(format!("/api/connections/{key}")))
            .send()
            .await
    }

    pub async fn set_connection_url(
        &self,
        connection: &ConnectionJson,
        url: &str,
    ) -> Outcome<ConnectionJson> {
        self.signed(
            self.app
                .api
                .patch(format!("/api/connections/{}", connection.key())),
        )
        .if_match(connection.version())
        .json(&json!({ "url": url }))
        .send()
        .await
    }

    pub async fn set_connection_secret(
        &self,
        connection: &ConnectionJson,
        name: &str,
        value: &str,
    ) -> Outcome<ConnectionJson> {
        self.signed(self.app.api.put(format!(
            "/api/connections/{}/secrets/{name}",
            connection.key()
        )))
        .if_match(connection.version())
        .json(&json!({ "value": value }))
        .send()
        .await
    }

    /// A conversation with the assistant, the user's own, empty.
    pub async fn start_conversation(&self) -> Outcome<serde_json::Value> {
        self.signed(self.app.api.post("/api/assistant/conversations"))
            .send()
            .await
    }

    /// Says `text` to the assistant in `conversation`, as last read: the turn's events, once
    /// it has ended.
    pub async fn ask_assistant(
        &self,
        conversation: &serde_json::Value,
        text: &str,
    ) -> Outcome<Vec<serde_json::Value>> {
        let id = conversation["id"]
            .as_str()
            .expect("a conversation has an id");
        let version: i64 = conversation["version"]
            .as_str()
            .and_then(|version| version.parse().ok())
            .expect("a conversation has a version");
        self.signed(
            self.app
                .api
                .post(format!("/api/assistant/conversations/{id}/messages")),
        )
        .if_match(version)
        .json(&json!({ "text": text }))
        .stream()
        .await
    }

    /// Approves the assistant's tool call `call` in conversation `id`: the resumed turn's
    /// events, once it has ended.
    pub async fn approve(
        &self,
        id: impl Display,
        call: impl Display,
    ) -> Outcome<Vec<serde_json::Value>> {
        self.signed(self.app.api.post(format!(
            "/api/assistant/conversations/{id}/tool-calls/{call}/decision"
        )))
        .json(&json!({ "decision": "approve" }))
        .stream()
        .await
    }

    pub async fn conversation(&self, id: impl Display) -> Outcome<serde_json::Value> {
        self.signed(
            self.app
                .api
                .get(format!("/api/assistant/conversations/{id}")),
        )
        .send()
        .await
    }

    pub async fn me(&self) -> Outcome<UserJson> {
        self.signed(self.app.api.get("/api/users/me")).send().await
    }

    /// The note's author is the caller.
    pub async fn write_note(&self, title: &str, body: &str) -> Outcome<NoteJson> {
        let request = json!({ "title": title, "body": body });
        self.signed(self.app.api.post("/api/notes"))
            .json(&request)
            .send()
            .await
    }

    /// At the version it reads at now.
    pub async fn edit_note(&self, id: impl Display, title: &str, body: &str) -> Outcome<NoteJson> {
        let note: Outcome<NoteJson> = self
            .signed(self.app.api.get(format!("/api/notes/{id}")))
            .send()
            .await;
        let note = note?;
        self.signed(self.app.api.patch(format!("/api/notes/{id}")))
            .if_match(note.version())
            .json(&json!({ "title": title, "body": body }))
            .send()
            .await
    }

    pub async fn note(&self, id: impl Display) -> Outcome<NoteJson> {
        self.signed(self.app.api.get(format!("/api/notes/{id}")))
            .send()
            .await
    }

    pub async fn notes(&self) -> Outcome<Page<NoteJson>> {
        self.signed(self.app.api.get("/api/notes")).send().await
    }

    /// The roles grants are made to: `access`' own, `admin`'s unless granted.
    pub async fn roles(&self) -> Outcome<Page<serde_json::Value>> {
        self.signed(self.app.api.get("/api/access/roles"))
            .send()
            .await
    }

    /// At the version the user's profile reads at now.
    pub async fn rename_myself(&self, display_name: &str) -> Outcome<UserJson> {
        let me = self.me().await?;
        self.signed(self.app.api.patch("/api/users/me"))
            .if_match(me.version())
            .json(&json!({ "displayName": display_name }))
            .send()
            .await
    }

    /// At the version the user's profile reads at now.
    pub async fn delete_account(&self) -> Outcome<()> {
        let me = self.me().await?;
        self.signed(self.app.api.delete("/api/users/me"))
            .if_match(me.version())
            .send_empty()
            .await
    }

    fn signed<'c>(&self, call: Call<'c>) -> Call<'c> {
        call.bearer(&self.token)
    }
}

/// Every journey's database starts as a copy of this: every module migrated,
/// its default grants applied, and seeded, as `migrate` and `seed` leave it,
/// with `READER` assigned `admin`.
pub const TEMPLATE: Template = Template::new("journeys", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &server::module_migrations::<Reference>())
            .await
            .expect("migrate every module");
        server::apply_default_grants::<Reference>(&Context::new(
            pool.clone(),
            Arc::new(app_domain::SystemClock),
        ))
        .await
        .expect("apply every module's default grants");
        let ctx = Context::new(pool.clone(), Arc::new(app_domain::SystemClock));
        server::declare_connections::<Reference>(&ctx)
            .await
            .expect("record every module's Connections");
        for module in Reference::modules() {
            module.seed(&ctx).await.expect("seed every module");
        }
        assign(&ctx, READER, ADMIN).await;
        // As in an environment seeded before this build: its subscriptions
        // receive none of the seeds' events, but an `earliest` one's backfill does.
        app_db::Worker::new(
            pool.clone(),
            Arc::new(app_domain::SystemClock),
            Default::default(),
        )
        .relay()
        .await
        .expect("relay the seeds' events");
    })
});

/// The scope `/mcp` asks of a token in the journeys served over the network:
/// a Cognito resource server's custom scope (`APP_MCP_SCOPE`).
pub const MCP_SCOPE: &str = "https://api.example.com/mcp";

/// The composed app served over TCP as `server` is in an environment with an
/// issuer: verifying `fake-oidc`'s tokens through its JWKS, with `APP_MCP_SCOPE`
/// set, for a client that discovers how to sign in as a real one would.
pub struct Served {
    /// Where the server is reached, without a trailing `/`.
    pub url: String,
    pub provider: Arc<app_fake_oidc::Provider>,
}

impl App {
    /// Serves this instance's database behind `fake-oidc`, signing in as `user`.
    pub async fn served_behind_an_issuer(&self, user: impl Display) -> Served {
        let (acceptor, issuer) = bind().await;
        let subject = app_fake_oidc::Subject {
            sub: user.to_string(),
            name: "Agent".into(),
            email: "agent@example.com".into(),
        };
        let provider = Arc::new(app_fake_oidc::Provider::new(issuer, "spa", subject));
        tokio::spawn(poem::Server::new_with_acceptor(acceptor).run(provider.clone().endpoint()));
        let verifier = JwtVerifier::from_discovery(provider.issuer(), None)
            .await
            .expect("the issuer's JWKS is read");
        let (acceptor, url) = bind().await;
        let authorizer = server::authorizer::<Reference>().expect("every module's policies load");
        let ctx = Context::new(self.pool.clone(), self.clock.clone())
            .with_authorizer(Arc::new(authorizer));
        let discovery = Discovery::new(
            app_web::PublicUrl::new(Some(url.clone())),
            Some(provider.issuer().to_owned()),
        )
        .with_scope(Some(MCP_SCOPE.to_owned()));
        let app =
            server::build_app::<Reference>(&ctx, verifier, discovery, None, "http://localhost");
        tokio::spawn(poem::Server::new_with_acceptor(acceptor).run(app));
        Served { url, provider }
    }
}

/// A listener on a free loopback port, and its URL.
pub async fn bind() -> (poem::listener::TcpAcceptor, String) {
    use poem::listener::{Acceptor, Listener, TcpListener};
    let acceptor = TcpListener::bind("127.0.0.1:0")
        .into_acceptor()
        .await
        .expect("binds");
    let addr = acceptor.local_addr()[0]
        .as_socket_addr()
        .copied()
        .expect("a socket address");
    (acceptor, format!("http://{addr}"))
}
