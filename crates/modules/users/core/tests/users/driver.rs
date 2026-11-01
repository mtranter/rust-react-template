//! The only place in these tests that knows the transport (spec §13).

use app_db::{DeadLetter, Worker};
use app_domain::{Clock, FixedClock, SystemClock, seed_time};
use app_error::{AppError, AppResult};
use app_events::{EnvelopeOf, Event, EventPayload, NoContext, PayloadOf};
use app_module::Context;
use app_persistence::Version;
use app_web::testing::{Call, Page, Refused, TestApi};
use app_web::{Accept, JwtVerifier, json_newtype};
use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use poem::EndpointExt;
use serde_json::json;
use sqlx::PgPool;
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

pub struct Users {
    api: TestApi,
    verifier: JwtVerifier,
    worker: Worker,
    clock: Arc<FixedClock>,
    mailer: Arc<FakeMailer>,
}

impl Users {
    pub async fn seeded(pool: PgPool) -> Self {
        migrate_and_seed(&pool).await;

        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let ctx = Context::new(pool, clock.clone());
        let mailer = Arc::new(FakeMailer::default());
        let handlers = users::handlers(&ctx, mailer.clone());
        app_db::register_subscriptions(&ctx.pool(), &handlers)
            .await
            .expect("register users' subscriptions");
        let worker = ctx.worker(handlers);
        let verifier = JwtVerifier::new("test-secret");
        let api = TestApi::new(UsersEndpoints, |app| {
            app.accept(users::provider(&ctx)).data(verifier.clone())
        });
        Self {
            api,
            verifier,
            worker,
            clock,
            mailer,
        }
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
        self.worker.published::<E>().await.expect("read the outbox")
    }

    pub async fn register(&self, email: &str, display_name: &str) -> Outcome<UserJson> {
        let request = json!({ "email": email, "displayName": display_name });
        self.api.post("/api/users").json(&request).send().await
    }

    pub async fn get(&self, id: impl Display) -> Outcome<UserJson> {
        self.api.get(format!("/api/users/{id}")).send().await
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
        self.api.get("/api/users").queries(query).send().await
    }

    /// The user need not exist.
    pub fn signed_in_as(&self, user: UserId) -> Session<'_> {
        let token = self
            .verifier
            .issue(user.to_string(), 60)
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
    migrate_and_seed(&pool).await;
    users::directory(&Context::new(pool, Arc::new(SystemClock)))
}

async fn migrate_and_seed(pool: &PgPool) {
    app_db::run_migrations(pool, &[users::migrations()])
        .await
        .expect("migrate users");
    seed::standard(pool).await.expect("seed users");
}
