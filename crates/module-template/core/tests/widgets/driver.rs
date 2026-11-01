//! The only place in these tests that knows the transport (spec §13).

use app_db::Worker;
use app_domain::{FixedClock, seed_time};
use app_events::{Event, EventPayload, NoContext, PayloadOf};
use app_module::Context;
use app_persistence::{Handlers, Version};
use app_web::testing::{Page, Refused, TestApi};
use app_web::{Accept, json_newtype};
use serde_json::json;
use sqlx::PgPool;
use std::fmt::Display;
use std::sync::Arc;
use widgets::{Widget, WidgetId, WidgetsEndpoints, seed};
use widgets_api::events::v1;

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A rule was broken: the paths of the inputs at fault, if it names any.
    Invalid(Vec<String>),
    /// The request couldn't be parsed.
    Malformed,
    NotFound,
    PreconditionRequired,
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
            "not_found" => Refusal::NotFound,
            "precondition_required" => Refusal::PreconditionRequired,
            "precondition_failed" => Refusal::Stale,
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

pub struct Widgets {
    api: TestApi,
    worker: Worker,
}

impl Widgets {
    pub async fn seeded(pool: PgPool) -> Self {
        app_db::run_migrations(&pool, &[widgets::migrations()])
            .await
            .expect("migrate widgets");
        seed::standard(&pool).await.expect("seed widgets");

        // Fixed after the seeds, so what a test creates is the newest.
        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let ctx = Context::new(pool, clock);
        let api = TestApi::new(WidgetsEndpoints, |app| app.accept(widgets::provider(&ctx)));
        // Widgets handles no events or jobs; the worker reads what it published.
        let worker = ctx.worker(Handlers::new());
        Self { api, worker }
    }

    /// Every `E` widgets has announced to other modules, oldest first, with the
    /// version of the widget each announces.
    pub async fn announced<E: Event>(&self) -> Vec<(Version, PayloadOf<E>)> {
        self.worker
            .published::<E>()
            .await
            .expect("read the outbox")
            .into_iter()
            .map(|event| (Version::from_i64(event.entity_version), event.data))
            .collect()
    }

    pub async fn get(&self, id: impl Display) -> Outcome<WidgetJson> {
        self.api.get(format!("/api/widgets/{id}")).send().await
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
        self.api.get("/api/widgets").queries(query).send().await
    }

    pub async fn create(&self, name: &str) -> Outcome<WidgetJson> {
        self.api
            .post("/api/widgets")
            .json(&json!({ "name": name }))
            .send()
            .await
    }

    pub async fn rename(&self, id: WidgetId, version: i64, name: &str) -> Outcome<WidgetJson> {
        self.api
            .patch(format!("/api/widgets/{id}"))
            .if_match(version)
            .json(&json!({ "name": name }))
            .send()
            .await
    }

    pub async fn rename_without_precondition(
        &self,
        id: WidgetId,
        name: &str,
    ) -> Outcome<WidgetJson> {
        self.api
            .patch(format!("/api/widgets/{id}"))
            .json(&json!({ "name": name }))
            .send()
            .await
    }
}
