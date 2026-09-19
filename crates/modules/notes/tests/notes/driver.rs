//! Test driver: the notes module's behaviour as domain verbs and outcomes. The
//! only place in these tests that knows the transport (routes, JSON, statuses).

use app_web::Accept;
use notes::{Note, NoteId, NotesApi, seed};
use poem::http::StatusCode;
use poem::test::{TestClient, TestResponse};
use poem::{EndpointExt, Route, endpoint::BoxEndpoint};
use poem_openapi::OpenApiService;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::fmt::Display;
use std::ops::Deref;

/// Why the module refused a request.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The input broke a rule (e.g. a blank title).
    Invalid,
    /// The thing asked for does not exist.
    NotFound,
    /// A write arrived without the required `If-Match` version.
    PreconditionRequired,
    /// The `If-Match` version was stale — the note changed underneath the caller.
    Stale,
}

pub type Outcome<T> = Result<T, Refusal>;

/// A note as a client sees it. Wraps the returned JSON: compare it whole against
/// a seed (`Ok(seed::first_note().into())`), or assert on parts of it by path
/// (`assert_json_matches!(note, "$.title", "Third")`) when some properties, like
/// a generated id, can't be predicted.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(transparent)]
pub struct NoteView(Value);

impl NoteView {
    pub fn id(&self) -> &str {
        self.0["id"].as_str().expect("a note has an id")
    }
}

impl Deref for NoteView {
    type Target = Value;
    fn deref(&self) -> &Value {
        &self.0
    }
}

impl From<Note> for NoteView {
    /// Builds the expected view of a note. Seeded and freshly-created notes are
    /// always at version `1`, so that is what a plain `Note` maps to; edited
    /// notes are compared against the edit's response, which carries its own.
    fn from(n: Note) -> Self {
        Self(json!({
            "id": n.id().to_string(),
            "author_id": n.author_id().to_string(),
            "title": n.title(),
            "body": n.body(),
            "created_at": n.created_at().to_rfc3339(),
            "version": "1",
        }))
    }
}

/// One page of notes plus the total across all pages.
#[derive(Debug, Deserialize)]
pub struct NotesPage {
    pub items: Vec<NoteView>,
    pub total: i64,
}

impl NotesPage {
    pub fn ids(&self) -> Vec<String> {
        self.items.iter().map(|n| n.id().to_string()).collect()
    }
}

pub struct Notes {
    client: TestClient<BoxEndpoint<'static>>,
}

impl Notes {
    /// The notes module running over `pool`, migrated and holding the standard
    /// seed dataset.
    pub async fn seeded(pool: PgPool) -> Self {
        app_db::run_migrations(&pool, &[notes::migrations()])
            .await
            .expect("migrate notes");
        seed::standard(&pool).await.expect("seed notes");

        let api = OpenApiService::new(NotesApi, "notes", "1.0");
        let app = Route::new()
            .nest("/api", api)
            .accept(notes::provider(pool))
            .boxed();
        Self {
            client: TestClient::new(app),
        }
    }

    pub async fn get(&self, id: impl Display) -> Outcome<NoteView> {
        outcome(self.client.get(format!("/api/notes/{id}")).send().await).await
    }

    pub async fn list(&self) -> NotesPage {
        outcome(self.client.get("/api/notes").send().await)
            .await
            .expect("listing notes never fails")
    }

    pub async fn create(&self, author: impl Display, title: &str, body: &str) -> Outcome<NoteView> {
        let request = json!({ "author_id": author.to_string(), "title": title, "body": body });
        outcome(
            self.client
                .post("/api/notes")
                .body_json(&request)
                .send()
                .await,
        )
        .await
    }

    /// Edit a note, asserting it is still at `version` (the `If-Match` precondition).
    pub async fn edit(
        &self,
        id: NoteId,
        version: i64,
        title: &str,
        body: &str,
    ) -> Outcome<NoteView> {
        let request = json!({ "title": title, "body": body });
        let response = self
            .client
            .patch(format!("/api/notes/{id}"))
            .header("If-Match", format!("\"{version}\""))
            .body_json(&request)
            .send()
            .await;
        outcome(response).await
    }

    /// Edit a note without stating which version — the missing-precondition path.
    pub async fn edit_without_precondition(
        &self,
        id: NoteId,
        title: &str,
        body: &str,
    ) -> Outcome<NoteView> {
        let request = json!({ "title": title, "body": body });
        outcome(
            self.client
                .patch(format!("/api/notes/{id}"))
                .body_json(&request)
                .send()
                .await,
        )
        .await
    }
}

/// Translate a response into a domain outcome.
async fn outcome<T: DeserializeOwned>(response: TestResponse) -> Outcome<T> {
    match response.0.status() {
        StatusCode::OK => Ok(response
            .0
            .into_body()
            .into_json()
            .await
            .expect("response body matches the expected shape")),
        StatusCode::BAD_REQUEST => Err(Refusal::Invalid),
        StatusCode::NOT_FOUND => Err(Refusal::NotFound),
        StatusCode::PRECONDITION_REQUIRED => Err(Refusal::PreconditionRequired),
        StatusCode::PRECONDITION_FAILED => Err(Refusal::Stale),
        other => panic!("unexpected status {other}"),
    }
}
