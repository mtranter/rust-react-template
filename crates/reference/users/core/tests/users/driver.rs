//! The only place in these tests that knows the transport (ADR 0004).

use app_authz::{Authorizer, Rows};
use app_db::{Clause, Columns, DeadLetter, Worker};
use app_domain::{Clock, FixedClock, SystemClock, seed_time};
use app_error::{AppError, AppResult};
use app_events::{EnvelopeOf, Event, EventPayload, NoContext, PayloadOf};
use app_module::Context;
use app_persistence::Version;
use app_test_db::Template;
use app_web::testing::{Call, Page, Refused, TestApi};
use app_web::{Accept, JwtVerifier, json_newtype};
use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use poem::EndpointExt;
use serde_json::json;
use sqlx::{PgPool, QueryBuilder};
use std::fmt::Display;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use users::{Mail, Mailer, User, UserId, UsersEndpoints, seed};
use users_api::UserDirectory;
use users_api::events::v1;

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
    /// The `If-Match` version was stale.
    Stale,
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
            "malformed_request" => Refusal::Malformed,
            "unauthorized" => Refusal::NotSignedIn,
            "forbidden" => Refusal::Forbidden,
            "not_found" => Refusal::NotFound,
            "conflict" => Refusal::Conflict,
            "precondition_required" => Refusal::PreconditionRequired,
            "precondition_failed" => Refusal::Stale,
            other => panic!("unexpected error code {other}"),
        }
    }
}

/// What a users event carries about `user`.
pub fn announced(user: &User) -> EventPayload<v1::User, NoContext> {
    EventPayload::new(v1::User {
        id: user.id().as_uuid(),
        email: user.email().to_string(),
        display_name: user.display_name().to_string(),
    })
}

pub type Outcome<T> = Result<T, Refusal>;

json_newtype!(pub UserJson);

impl From<User> for UserJson {
    /// Version `1`: seeded and freshly-registered users are always at it.
    fn from(u: User) -> Self {
        Self(json!({
            "id": u.id().to_string(),
            "email": u.email().as_str(),
            "displayName": u.display_name(),
            "createdAt": u.created_at().to_rfc3339(),
            "version": "1",
        }))
    }
}

pub type UsersPage = Page<UserJson>;

/// Stands in for the mail server, which tests can't run: keeps what it's sent,
/// and refuses everything while it's down.
#[derive(Default)]
struct FakeMailer {
    sent: Mutex<Vec<Mail>>,
    down: AtomicBool,
}

#[async_trait]
impl Mailer for FakeMailer {
    async fn send(&self, mail: Mail) -> AppResult<()> {
        if self.down.load(Ordering::SeqCst) {
            return Err(AppError::internal(std::io::Error::other(
                "the mail server is down",
            )));
        }
        self.sent.lock().unwrap().push(mail);
        Ok(())
    }
}

/// The role every signed-in user has (ADR 0040), which users' default grants name.
pub const USER: &str = "user";

pub struct Users {
    api: TestApi,
    ctx: Context,
    verifier: JwtVerifier,
    /// What reads outside a session present: Alice's, a `user`.
    reader: String,
    worker: Worker,
    clock: Arc<FixedClock>,
    mailer: Arc<FakeMailer>,
    seeded_through: i64,
}

impl Users {
    pub async fn seeded(pool: PgPool) -> Self {
        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let policies = users::policies().build().expect("users' policies load");
        let authorizer = Authorizer::from_policies(&[policies]).expect("users' policies compile");
        let ctx = Context::new(pool, clock.clone()).with_authorizer(Arc::new(authorizer));
        let mailer = Arc::new(FakeMailer::default());
        let handlers = users::handlers(&ctx, mailer.clone());
        app_db::register_subscriptions(&ctx.pool(), clock.as_ref(), &handlers)
            .await
            .expect("register users' subscriptions");
        let worker = ctx.worker(handlers);
        let seeded_through = worker.outbox_end().await.expect("read the outbox");
        let verifier = JwtVerifier::new("test-secret");
        let api = TestApi::new(UsersEndpoints, ctx.idempotency(), |app| {
            app.accept(users::provider(&ctx))
                .data(verifier.clone())
                .data(ctx.authorizer())
        });
        let reader = verifier
            .issue(seed::USER_ID_ALICE.to_string(), &[USER], 60)
            .expect("issue token");
        Self {
            api,
            ctx,
            verifier,
            reader,
            worker,
            clock,
            mailer,
            seeded_through,
        }
    }

    /// The rule set the endpoints are served under.
    pub fn authorizer(&self) -> Arc<Authorizer> {
        self.ctx.authorizer()
    }

    /// The ids of the stored accounts `rows` holds, queried as the
    /// repository's list scopes it, through `C`'s columns.
    pub async fn ids_where<C: Columns>(&self, rows: Rows) -> Vec<uuid::Uuid> {
        let mut sql = QueryBuilder::new("SELECT id FROM users.account");
        app_db::push_scope::<C>(&mut sql, Clause::Where, &rows);
        sql.build_query_scalar()
            .fetch_all(&self.ctx.pool())
            .await
            .expect("query the accounts")
    }

    /// Delivers the events announced so far, as the worker would.
    pub async fn run_pending_events(&self) {
        self.worker
            .run_until_idle()
            .await
            .expect("the worker reaches the database");
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    pub fn later(&self, by: TimeDelta) {
        self.clock.advance(by);
    }

    pub fn mail_sent(&self) -> Vec<Mail> {
        self.mailer.sent.lock().unwrap().clone()
    }

    pub fn mail_server_goes_down(&self) {
        self.mailer.down.store(true, Ordering::SeqCst);
    }

    pub fn mail_server_comes_back(&self) {
        self.mailer.down.store(false, Ordering::SeqCst);
    }

    /// Background work that ran out of attempts.
    pub async fn given_up_on(&self) -> Vec<DeadLetter> {
        self.worker.dead_letters().await.expect("read dead letters")
    }

    /// Every `E` users has announced to other modules, oldest first, with the
    /// version of the user each announces.
    pub async fn announced<E: Event>(&self) -> Vec<(Version, PayloadOf<E>)> {
        self.announced_envelopes::<E>()
            .await
            .into_iter()
            .map(|event| (Version::from_i64(event.entity_version), event.data))
            .collect()
    }

    pub async fn announced_envelopes<E: Event>(&self) -> Vec<EnvelopeOf<E>> {
        self.worker
            .published_after::<E>(self.seeded_through)
            .await
            .expect("read the outbox")
    }

    pub async fn register(&self, email: &str, display_name: &str) -> Outcome<UserJson> {
        let request = json!({ "email": email, "displayName": display_name });
        self.api.post("/api/users").json(&request).send().await
    }

    pub async fn get(&self, id: impl Display) -> Outcome<UserJson> {
        self.api
            .get(format!("/api/users/{id}"))
            .bearer(&self.reader)
            .send()
            .await
    }

    pub async fn list(&self) -> UsersPage {
        self.list_with(&[])
            .await
            .expect("listing users never fails")
    }

    /// `sort` keys as the API names them, `-` prefix for descending.
    pub async fn list_sorted(&self, sort: &[&str]) -> Outcome<UsersPage> {
        let query: Vec<_> = sort.iter().map(|key| ("sort", key.to_string())).collect();
        self.list_with(&query).await
    }

    pub async fn list_page(&self, limit: i64, offset: i64) -> UsersPage {
        self.list_with(&[("limit", limit.to_string()), ("offset", offset.to_string())])
            .await
            .expect("listing users never fails")
    }

    async fn list_with(&self, query: &[(&str, String)]) -> Outcome<UsersPage> {
        self.api
            .get("/api/users")
            .bearer(&self.reader)
            .queries(query)
            .send()
            .await
    }

    /// As a `user`, who need not exist.
    pub fn signed_in_as(&self, user: UserId) -> Session<'_> {
        self.signed_in_with_roles(user, &[USER])
    }

    pub fn signed_in_with_roles(&self, user: UserId, roles: &[&str]) -> Session<'_> {
        let token = self
            .verifier
            .issue(user.to_string(), roles, 60)
            .expect("issue token");
        Session {
            users: self,
            token: Some(token),
        }
    }

    pub fn anonymous(&self) -> Session<'_> {
        Session {
            users: self,
            token: None,
        }
    }
}

pub struct Session<'a> {
    users: &'a Users,
    token: Option<String>,
}

impl Session<'_> {
    pub async fn me(&self) -> Outcome<UserJson> {
        self.signed(self.users.api.get("/api/users/me"))
            .send()
            .await
    }

    pub async fn change_display_name(&self, version: i64, display_name: &str) -> Outcome<UserJson> {
        self.signed(self.users.api.patch("/api/users/me"))
            .if_match(version)
            .json(&json!({ "displayName": display_name }))
            .send()
            .await
    }

    pub async fn change_display_name_without_precondition(
        &self,
        display_name: &str,
    ) -> Outcome<UserJson> {
        self.signed(self.users.api.patch("/api/users/me"))
            .json(&json!({ "displayName": display_name }))
            .send()
            .await
    }

    pub async fn delete_account(&self, version: i64) -> Outcome<()> {
        self.signed(self.users.api.delete("/api/users/me"))
            .if_match(version)
            .send_empty()
            .await
    }

    /// As [`change_display_name`](Self::change_display_name), sent with `key`
    /// as its `Idempotency-Key`.
    pub async fn change_display_name_keyed(
        &self,
        key: &str,
        version: i64,
        display_name: &str,
    ) -> Outcome<UserJson> {
        self.signed(self.users.api.patch("/api/users/me"))
            .idempotency_key(key)
            .if_match(version)
            .json(&json!({ "displayName": display_name }))
            .send()
            .await
    }

    pub async fn delete_account_without_precondition(&self) -> Outcome<()> {
        self.signed(self.users.api.delete("/api/users/me"))
            .send_empty()
            .await
    }

    fn signed<'c>(&self, call: Call<'c>) -> Call<'c> {
        match &self.token {
            Some(token) => call.bearer(token),
            None => call,
        }
    }
}

/// Tested directly: the directory has no HTTP entry point.
pub async fn seeded_directory(pool: PgPool) -> Arc<dyn UserDirectory> {
    users::directory(&Context::new(pool, Arc::new(SystemClock)))
}

/// Every test's database starts as a copy of this: migrated and seeded.
pub const TEMPLATE: Template = Template::new("users", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &[users::migrations()])
            .await
            .expect("migrate users");
        seed::standard(&pool).await.expect("seed users");
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
