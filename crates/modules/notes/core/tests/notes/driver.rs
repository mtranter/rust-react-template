use app_error::AppResult;
use app_web::testing::{Page, Refused, TestApi};
use app_web::{Accept, json_newtype};
use async_trait::async_trait;
use notes::{Note, NoteId, NotesEndpoints, seed};
use serde_json::json;
use sqlx::PgPool;
use std::fmt::Display;
use std::sync::Arc;
use users_api::UserDirectory;
use uuid::Uuid;

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

json_newtype!(pub NoteJson);

impl From<Note> for NoteJson {
    /// Seeded and freshly-created notes are always at version `1`; edited notes
    /// are compared against the edit's response instead.
    fn from(n: Note) -> Self {
        Self(json!({
            "id": n.id().to_string(),
            "authorId": n.author_id().to_string(),
            "title": n.title(),
            "body": n.body(),
            "createdAt": n.created_at().to_rfc3339(),
            "version": "1",
        }))
    }
}

pub type NotesPage = Page<NoteJson>;

/// Stands in for the users module, which these tests don't run; the users module
/// tests its real `UserDirectory` against Postgres. Knows the users the seeded
/// notes are by.
struct KnownUsers;

#[async_trait]
impl UserDirectory for KnownUsers {
    async fn exists(&self, id: Uuid) -> AppResult<bool> {
        Ok([users_api::seed::ALICE, users_api::seed::BOB].contains(&id))
    }
}

pub struct Notes {
    api: TestApi,
}

impl Notes {
    pub async fn seeded(pool: PgPool) -> Self {
        app_db::run_migrations(&pool, &[notes::migrations()])
            .await
            .expect("migrate notes");
        seed::standard(&pool).await.expect("seed notes");

        let api = TestApi::new(NotesEndpoints, |app| {
            app.accept(notes::provider(pool, Arc::new(KnownUsers)))
        });
        Self { api }
    }

    pub async fn get(&self, id: impl Display) -> Outcome<NoteJson> {
        self.api.get(format!("/api/notes/{id}")).send().await
    }

    pub async fn list(&self) -> NotesPage {
        self.list_with(&[])
            .await
            .expect("listing notes never fails")
    }

    /// `sort` keys as the API names them, `-` prefix for descending.
    pub async fn list_sorted(&self, sort: &[&str]) -> Outcome<NotesPage> {
        let query: Vec<_> = sort.iter().map(|key| ("sort", key.to_string())).collect();
        self.list_with(&query).await
    }

    pub async fn list_page(&self, limit: i64, offset: i64) -> NotesPage {
        self.list_with(&[("limit", limit.to_string()), ("offset", offset.to_string())])
            .await
            .expect("listing notes never fails")
    }

    async fn list_with(&self, query: &[(&str, String)]) -> Outcome<NotesPage> {
        self.api.get("/api/notes").queries(query).send().await
    }

    pub async fn create(&self, author: impl Display, title: &str, body: &str) -> Outcome<NoteJson> {
        let request = json!({ "authorId": author.to_string(), "title": title, "body": body });
        self.api.post("/api/notes").json(&request).send().await
    }

    /// The body omits `title` entirely, rather than sending it blank.
    pub async fn create_without_a_title(&self, author: impl Display) -> Outcome<NoteJson> {
        let request = json!({ "authorId": author.to_string(), "body": "" });
        self.api.post("/api/notes").json(&request).send().await
    }

    pub async fn edit(
        &self,
        id: NoteId,
        version: i64,
        title: &str,
        body: &str,
    ) -> Outcome<NoteJson> {
        let request = json!({ "title": title, "body": body });
        self.api
            .patch(format!("/api/notes/{id}"))
            .if_match(version)
            .json(&request)
            .send()
            .await
    }

    pub async fn edit_without_precondition(
        &self,
        id: NoteId,
        title: &str,
        body: &str,
    ) -> Outcome<NoteJson> {
        let request = json!({ "title": title, "body": body });
        self.api
            .patch(format!("/api/notes/{id}"))
            .json(&request)
            .send()
            .await
    }
}
