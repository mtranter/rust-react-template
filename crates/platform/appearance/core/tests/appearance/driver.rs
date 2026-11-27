//! The only place in these tests that knows the transport (ADR 0004).

use app_authz::Authorizer;
use app_db::Worker;
use app_domain::{FixedClock, seed_time};
use app_events::{Event, PayloadOf};
use app_module::Context;
use app_persistence::Version;
use app_test_db::Template;
use app_web::testing::{Call, Refused, TestApi};
use app_web::{Accept, JwtVerifier};
use appearance::AppearanceEndpoints;
use poem::EndpointExt;
use serde_json::Value;
use sqlx::PgPool;
use std::sync::{Arc, Mutex};

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The paths of the inputs at fault.
    Invalid(Vec<String>),
    NotSignedIn,
    Forbidden,
    PreconditionRequired,
    Stale,
}

impl From<Refused> for Refusal {
    fn from(refused: Refused) -> Self {
        match refused.code.as_str() {
            "validation_error" => Refusal::Invalid(refused.paths),
            "unauthorized" => Refusal::NotSignedIn,
            "forbidden" => Refusal::Forbidden,
            "precondition_required" => Refusal::PreconditionRequired,
            "precondition_failed" => Refusal::Stale,
            other => panic!("unexpected error code {other}"),
        }
    }
}

pub type Outcome<T> = Result<T, Refusal>;

pub const ADMIN: &str = "admin";
pub const SUBJECT: &str = "a-user";

pub struct Appearance {
    api: TestApi,
    ctx: Context,
    worker: Worker,
    seeded_through: i64,
    verifier: JwtVerifier,
    caller: Mutex<Option<String>>,
}

impl Appearance {
    /// Signed in as an `admin`.
    pub async fn seeded(pool: PgPool) -> Self {
        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let policies = appearance::policies()
            .build()
            .expect("appearance's policies load");
        let authorizer =
            Authorizer::from_policies(&[policies]).expect("appearance's policies compile");
        let ctx = Context::new(pool, clock).with_authorizer(Arc::new(authorizer));
        let verifier = JwtVerifier::new("test-secret");
        let api = TestApi::new(AppearanceEndpoints, ctx.idempotency(), |app| {
            app.accept(appearance::provider(&ctx))
                .data(verifier.clone())
                .data(ctx.authorizer())
        });
        let worker = ctx.worker(Default::default());
        let seeded_through = worker.outbox_end().await.expect("read the outbox");
        let appearance = Self {
            api,
            ctx,
            worker,
            seeded_through,
            verifier,
            caller: Mutex::new(None),
        };
        appearance.sign_in_with_roles(&[ADMIN]);
        appearance
    }

    pub fn sign_in_with_roles(&self, roles: &[&str]) {
        self.ctx.authorizer().assign_in_memory(SUBJECT, roles);
        let token = self.verifier.issue(SUBJECT, 60).expect("issue token");
        *self.caller.lock().unwrap() = Some(token);
    }

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

    pub async fn theme(&self) -> Outcome<Value> {
        self.signed(self.api.get("/api/appearance/theme"))
            .send()
            .await
    }

    /// `theme` as the API takes it: `light`, `dark`, `shared`.
    pub async fn update(&self, version: i64, theme: Value) -> Outcome<Value> {
        self.signed(self.api.put("/api/appearance/theme"))
            .if_match(version)
            .json(&theme)
            .send()
            .await
    }

    pub async fn update_without_precondition(&self, theme: Value) -> Outcome<Value> {
        self.signed(self.api.put("/api/appearance/theme"))
            .json(&theme)
            .send()
            .await
    }

    pub async fn apps(&self) -> Outcome<Value> {
        self.signed(self.api.get("/api/appearance/apps"))
            .send()
            .await
    }

    pub async fn hide_apps(&self, version: i64, hidden: &[&str]) -> Outcome<Value> {
        self.signed(self.api.put("/api/appearance/apps"))
            .if_match(version)
            .json(&serde_json::json!({ "hidden": hidden }))
            .send()
            .await
    }

    /// Every `E` announced since boot, with the version each announces.
    pub async fn announced<E: Event>(&self) -> Vec<(Version, PayloadOf<E>)> {
        self.worker
            .published_after::<E>(self.seeded_through)
            .await
            .expect("read the outbox")
            .into_iter()
            .map(|event| (Version::from_i64(event.entity_version), event.data))
            .collect()
    }
}

pub const TEMPLATE: Template = Template::new("appearance", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &[appearance::migrations()])
            .await
            .expect("migrate appearance");
    })
});
