//! The only place in these tests that knows the transport (spec §13). Unlike a
//! write module's driver it stubs nothing: the users and notes modules' real
//! migrations publish the views this module reads, and their seeds fill them
//! (spec D8).

use app_domain::SystemClock;
use app_module::Context;
use app_web::testing::{Page, Refused, TestApi};
use app_web::{Accept, json_newtype};
use note_search::NoteSearchEndpoints;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The request couldn't be parsed.
    Malformed,
}

impl From<Refused> for Refusal {
    fn from(refused: Refused) -> Self {
        match refused.code.as_str() {
            "malformed_request" => Refusal::Malformed,
            other => panic!("unexpected error code {other}"),
        }
    }
}

pub type Outcome<T> = Result<T, Refusal>;

json_newtype!(pub NoteSummaryJson);

impl NoteSummaryJson {
    /// `author` is `None` for a note whose author id no user has.
    pub fn of(note: notes_api::v1::Note, author: Option<users_api::v1::User>) -> Self {
        let mut json = json!({
            "id": note.id.to_string(),
            "title": note.title,
            "authorId": note.author_id.to_string(),
            "createdAt": note.created_at.to_rfc3339(),
        });
        if let Some(author) = author {
            json["authorName"] = author.display_name.into();
        }
        Self(json)
    }
}

pub type Found = Page<NoteSummaryJson>;

pub struct NoteSearch {
    api: TestApi,
}

impl NoteSearch {
    pub async fn seeded(pool: PgPool) -> Self {
        app_db::run_migrations(&pool, &[users::migrations(), notes::migrations()])
            .await
            .expect("migrate the modules whose views are read");
        users::seed::standard(&pool).await.expect("seed users");
        notes::seed::standard(&pool).await.expect("seed notes");

        let api = TestApi::new(NoteSearchEndpoints, |app| {
            app.accept(note_search::provider(&Context::new(
                pool,
                Arc::new(SystemClock),
            )))
        });
        Self { api }
    }

    pub async fn all(&self) -> Found {
        self.search(&[]).await.expect("listing notes never fails")
    }

    pub async fn matching(&self, text: &str) -> Found {
        self.search(&[("q", text.to_string())])
            .await
            .expect("searching never fails")
    }

    pub async fn matching_page(&self, text: &str, limit: i64, offset: i64) -> Found {
        self.search(&[
            ("q", text.to_string()),
            ("limit", limit.to_string()),
            ("offset", offset.to_string()),
        ])
        .await
        .expect("searching never fails")
    }

    /// `sort` keys as the API names them, `-` prefix for descending.
    pub async fn sorted(&self, sort: &[&str]) -> Outcome<Found> {
        let query: Vec<_> = sort.iter().map(|key| ("sort", key.to_string())).collect();
        self.search(&query).await
    }

    async fn search(&self, query: &[(&str, String)]) -> Outcome<Found> {
        self.api.get("/api/note-search").queries(query).send().await
    }
}
