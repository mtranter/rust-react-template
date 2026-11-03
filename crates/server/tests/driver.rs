//! The only place in the journeys that knows the transport (spec §13). Unlike a
//! module's driver it stubs nothing: it boots the composed app as `server` does,
//! over every registered module, with the composed worker and real JWT auth.

use app_db::{DeadLetter, Worker};
use app_domain::{Clock, FixedClock, seed_time};
use app_module::Context;
use app_web::testing::{Call, Page, Refused, TestApi};
use app_web::{JwtVerifier, json_newtype};
use chrono::{DateTime, Utc};
use serde_json::json;
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
    NotFound,
    Conflict,
    PreconditionRequired,
    Stale,
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

pub type Outcome<T> = Result<T, Refusal>;

json_newtype!(pub UserJson);

json_newtype!(pub NoteJson);

impl NoteJson {
    /// As the note reads once archived at `at`, by one more save.
    pub fn archived(&self, at: DateTime<Utc>) -> Self {
        let mut json = self.0.clone();
        json["archivedAt"] = at.to_rfc3339().into();
        json["version"] = (self.version() + 1).to_string().into();
        Self(json)
    }
}

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

pub struct App {
    api: TestApi,
    verifier: JwtVerifier,
    worker: Worker,
    clock: Arc<FixedClock>,
}

impl App {
    /// Migrates, seeds and registers subscriptions for every module, as
    /// `migrate` and `seed` do, then builds the app and worker as `server` does.
    pub async fn seeded(pool: PgPool) -> Self {
        app_db::run_migrations(&pool, &server::module_migrations())
            .await
            .expect("migrate every module");
        for module in server::modules() {
            module.seed(pool.clone()).await.expect("seed every module");
        }

        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let ctx = Context::new(pool, clock.clone());
        server::register_subscriptions(&ctx)
            .await
            .expect("register every module's subscriptions");
        let worker = server::worker(&ctx);
        let verifier = JwtVerifier::new("test-secret");
        let api = TestApi::over(server::build_app(
            &ctx,
            verifier.clone(),
            "http://localhost",
        ));
        Self {
            api,
            verifier,
            worker,
            clock,
        }
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

    /// Background work that ran out of attempts.
    pub async fn given_up_on(&self) -> Vec<DeadLetter> {
        self.worker.dead_letters().await.expect("read dead letters")
    }

    pub async fn note(&self, id: impl Display) -> Outcome<NoteJson> {
        self.api.get(format!("/api/notes/{id}")).send().await
    }

    pub async fn search_notes(&self, text: &str) -> Found {
        let found: Outcome<Found> = self
            .api
            .get("/api/note-search")
            .query("q", text)
            .send()
            .await;
        found.expect("searching never fails")
    }

    /// Signed in with a real token, which the server verifies.
    pub fn as_user(&self, user: impl Display) -> Session<'_> {
        let token = self
            .verifier
            .issue(user.to_string(), 60)
            .expect("issue token");
        Session {
            app: self,
            user: user.to_string(),
            token,
        }
    }
}

pub struct Session<'a> {
    app: &'a App,
    user: String,
    token: String,
}

impl Session<'_> {
    pub async fn me(&self) -> Outcome<UserJson> {
        self.signed(self.app.api.get("/api/users/me")).send().await
    }

    /// Notes take their author from the request, not the token.
    pub async fn write_note(&self, title: &str, body: &str) -> Outcome<NoteJson> {
        let request = json!({ "authorId": self.user, "title": title, "body": body });
        self.app.api.post("/api/notes").json(&request).send().await
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
