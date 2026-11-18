//! The only place in these tests that knows the transport (ADR 0004).

use app_authz::{Authorizer, Rows};
use app_db::{Clause, Columns, Worker};
use app_domain::{FixedClock, seed_time};
use app_events::{Event, EventPayload, NoContext, PayloadOf};
use app_module::Context;
use app_persistence::Version;
use app_test_db::Template;
use app_web::testing::{Call, Page, Refused, TestApi};
use app_web::{Accept, JwtVerifier, json_newtype};
use poem::EndpointExt;
use serde_json::json;
use sqlx::{PgPool, QueryBuilder};
use std::fmt::Display;
use std::sync::{Arc, Mutex};
use widgets::{Widget, WidgetsEndpoints, seed};
use widgets_api::events::v1;

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
    PreconditionRequired,
    Stale,
    /// The search copy hasn't caught up with what widgets had published yet.
    Unavailable,
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
            "precondition_required" => Refusal::PreconditionRequired,
            "precondition_failed" => Refusal::Stale,
            "unavailable" => Refusal::Unavailable,
            other => panic!("unexpected error code {other}"),
        }
    }
}

pub type Outcome<T> = Result<T, Refusal>;

json_newtype!(pub WidgetJson);

impl From<Widget> for WidgetJson {
    /// Version `1`: seeded and freshly-created widgets are always at it.
    fn from(w: Widget) -> Self {
        Self(json!({
            "id": w.id().to_string(),
            "name": w.name(),
            "createdAt": w.created_at().to_rfc3339(),
            "version": "1",
        }))
    }
}

/// What a widgets event carries about `widget`.
pub fn announced(widget: &Widget) -> EventPayload<v1::Widget, NoContext> {
    EventPayload::new(v1::Widget {
        id: widget.id().as_uuid(),
        name: widget.name().to_string(),
    })
}

pub type WidgetsPage = Page<WidgetJson>;

json_newtype!(pub WidgetMatchJson);

impl From<Widget> for WidgetMatchJson {
    fn from(w: Widget) -> Self {
        Self(json!({ "id": w.id().to_string(), "name": w.name() }))
    }
}

pub type MatchesPage = Page<WidgetMatchJson>;

/// The role every signed-in user has (ADR 0040), which widgets' default grants name.
pub const USER: &str = "user";

pub struct Widgets {
    api: TestApi,
    ctx: Context,
    worker: Worker,
    seeded_through: i64,
    verifier: JwtVerifier,
    /// The token later requests present, if any.
    caller: Mutex<Option<String>>,
}

impl Widgets {
    pub async fn seeded(pool: PgPool) -> Self {
        // Fixed after the seeds, so what a test creates is the newest.
        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let policies = widgets::policies().build().expect("widgets' policies load");
        let authorizer = Authorizer::from_policies(&[policies]).expect("widgets' policies compile");
        let ctx = Context::new(pool, clock.clone()).with_authorizer(Arc::new(authorizer));
        let verifier = JwtVerifier::new("test-secret");
        let api = TestApi::new(WidgetsEndpoints, ctx.idempotency(), |app| {
            app.accept(widgets::provider(&ctx))
                .data(verifier.clone())
                .data(ctx.authorizer())
        });
        // Registered after the seeds, so the search copy starts from the backfill.
        let handlers = widgets::handlers(&ctx);
        app_db::register_subscriptions(&ctx.pool(), clock.as_ref(), &handlers)
            .await
            .expect("register the search copy's subscription");
        let worker = ctx.worker(handlers);
        let seeded_through = worker.outbox_end().await.expect("read the outbox");
        let widgets = Self {
            api,
            ctx,
            worker,
            seeded_through,
            verifier,
            caller: Mutex::new(None),
        };
        widgets.sign_in_with_roles(&[USER]);
        widgets
    }

    /// The rule set the endpoints are served under.
    pub fn authorizer(&self) -> Arc<Authorizer> {
        self.ctx.authorizer()
    }

    /// The ids of the stored widgets `rows` holds, queried as the repository's
    /// list scopes it, through `C`'s columns.
    pub async fn ids_where<C: Columns>(&self, rows: Rows) -> Vec<uuid::Uuid> {
        let mut sql = QueryBuilder::new("SELECT id FROM widgets.widget");
        app_db::push_scope::<C>(&mut sql, Clause::Where, &rows);
        sql.build_query_scalar()
            .fetch_all(&self.ctx.pool())
            .await
            .expect("query the widgets")
    }

    /// Later requests present a real token carrying `roles`. Every test
    /// starts signed in as a `user`.
    pub fn sign_in_with_roles(&self, roles: &[&str]) {
        let token = self
            .verifier
            .issue("a-user", roles, 60)
            .expect("issue token");
        *self.caller.lock().unwrap() = Some(token);
    }

    /// Later requests present no token.
    pub fn sign_out(&self) {
        *self.caller.lock().unwrap() = None;
    }

    fn signed<'c>(&self, call: Call<'c>) -> Call<'c> {
        match self.caller.lock().unwrap().as_ref() {
            Some(token) => call.bearer(token),
            None => call,
        }
    }

    /// Every `E` widgets has announced to other modules, oldest first, with the
    /// version of the widget each announces.
    pub async fn announced<E: Event>(&self) -> Vec<(Version, PayloadOf<E>)> {
        self.worker
            .published_after::<E>(self.seeded_through)
            .await
            .expect("read the outbox")
            .into_iter()
            .map(|event| (Version::from_i64(event.entity_version), event.data))
            .collect()
    }

    /// Delivers what widgets has published to its search copy.
    pub async fn run_pending_events(&self) {
        self.worker
            .run_until_idle()
            .await
            .expect("the worker reaches the database");
    }

    pub async fn search(&self, name: &str) -> Outcome<MatchesPage> {
        self.signed(self.api.get("/api/widgets/search"))
            .queries(&[("name", name.to_string())])
            .send()
            .await
    }

    pub async fn get(&self, id: impl Display) -> Outcome<WidgetJson> {
        self.signed(self.api.get(format!("/api/widgets/{id}")))
            .send()
            .await
    }

    pub async fn list(&self) -> WidgetsPage {
        self.list_with(&[])
            .await
            .expect("listing widgets never fails")
    }

    /// `sort` keys as the API names them, `-` prefix for descending.
    pub async fn list_sorted(&self, sort: &[&str]) -> Outcome<WidgetsPage> {
        let query: Vec<_> = sort.iter().map(|key| ("sort", key.to_string())).collect();
        self.list_with(&query).await
    }

    async fn list_with(&self, query: &[(&str, String)]) -> Outcome<WidgetsPage> {
        self.signed(self.api.get("/api/widgets").queries(query))
            .send()
            .await
    }

    pub async fn create(&self, name: &str) -> Outcome<WidgetJson> {
        self.signed(self.api.post("/api/widgets"))
            .json(&json!({ "name": name }))
            .send()
            .await
    }

    pub async fn rename(&self, id: impl Display, version: i64, name: &str) -> Outcome<WidgetJson> {
        self.signed(self.api.patch(format!("/api/widgets/{id}")))
            .if_match(version)
            .json(&json!({ "name": name }))
            .send()
            .await
    }

    pub async fn rename_without_precondition(
        &self,
        id: impl Display,
        name: &str,
    ) -> Outcome<WidgetJson> {
        self.signed(self.api.patch(format!("/api/widgets/{id}")))
            .json(&json!({ "name": name }))
            .send()
            .await
    }
}

/// Every test's database starts as a copy of this: migrated and seeded.
pub const TEMPLATE: Template = Template::new("widgets", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &[widgets::migrations()])
            .await
            .expect("migrate widgets");
        seed::standard(&pool).await.expect("seed widgets");
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
