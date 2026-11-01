use app_db::Worker;
use app_domain::{Clock, FixedClock, seed_time};
use app_error::AppResult;
use app_events::{Event, EventPayload, NoContext, PayloadOf};
use app_module::Context;
use app_persistence::PendingEvent;
use app_web::testing::{Page, Refused, TestApi};
use app_web::{Accept, json_newtype};
use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use notes::{Note, NoteId, NotesEndpoints, seed};
use notes_api::events::v1 as notes_events;
use serde_json::json;
use sqlx::{AssertSqlSafe, PgPool};
use std::fmt::Display;
use std::sync::Arc;
use users_api::UserDirectory;
use users_api::events::v1::{self as users_events, UserTopic};
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A rule was broken: the paths of the inputs at fault, if it names any.
    Invalid(Vec<String>),
    /// The request couldn't be parsed.
    Malformed,
    NotFound,
    Conflict,
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
            "conflict" => Refusal::Conflict,
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

impl NoteJson {
    /// As the note reads once archived at `at`, by one more save.
    pub fn archived(&self, at: DateTime<Utc>) -> Self {
        let mut json = self.0.clone();
        let version: i64 = json["version"].as_str().unwrap().parse().unwrap();
        json["archivedAt"] = at.to_rfc3339().into();
        json["version"] = (version + 1).to_string().into();
        Self(json)
    }
}

/// What a notes event carries about `note`.
pub fn announced(note: &Note) -> EventPayload<notes_events::Note, NoContext> {
    EventPayload::new(notes_events::Note {
        id: note.id().as_uuid(),
        author_id: note.author_id().as_uuid(),
        title: note.title().to_string(),
        body: note.body().to_string(),
        archived_at: note.archived_at(),
    })
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
    ctx: Context,
    clock: Arc<FixedClock>,
    worker: Worker,
}

impl Notes {
    pub async fn seeded(pool: PgPool) -> Self {
        app_db::run_migrations(&pool, &[notes::migrations()])
            .await
            .expect("migrate notes");
        seed::standard(&pool).await.expect("seed notes");

        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let ctx = Context::new(pool, clock.clone());
        let handlers = notes::handlers(&ctx, Arc::new(KnownUsers));
        app_db::register_subscriptions(&ctx.pool(), &handlers)
            .await
            .expect("register notes' subscriptions");
        let worker = ctx.worker(handlers);
        let api = TestApi::new(NotesEndpoints, |app| {
            app.accept(notes::provider(&ctx, Arc::new(KnownUsers)))
        });
        Self {
            api,
            ctx,
            clock,
            worker,
        }
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    pub fn later(&self, by: TimeDelta) {
        self.clock.advance(by);
    }

    /// The users module announcing a seeded user's deletion, as it would after
    /// `DELETE /users/me`: the event is the contract, so it stands in for running users.
    pub async fn user_deleted(&self, user: Uuid) {
        let user = [users_api::seed::alice(), users_api::seed::bob()]
            .into_iter()
            .find(|seeded| seeded.id == user)
            .expect("a seeded user");
        let mut tx = self.ctx.transactions().begin().await.expect("begin");
        self.ctx
            .publisher()
            .publish(
                &mut tx,
                PendingEvent::on::<UserTopic>(
                    &users_events::User {
                        id: user.id,
                        email: user.email,
                        display_name: user.display_name,
                    },
                    users_events::UserEvent::Deleted,
                ),
            )
            .await
            .expect("publish");
        tx.commit().await.expect("commit");
    }

    /// Every `E` notes has announced to other modules, oldest first.
    pub async fn announced<E: Event>(&self) -> Vec<PayloadOf<E>> {
        self.worker
            .published::<E>()
            .await
            .expect("read the outbox")
            .into_iter()
            .map(|event| event.data)
            .collect()
    }

    /// Delivers the events announced so far, as the worker would.
    pub async fn run_pending_events(&self) {
        self.worker
            .run_until_idle()
            .await
            .expect("the worker reaches the database");
    }

    /// Fault injection: any save of `note` fails, as on a database error, until
    /// [`saving_works_again`](Self::saving_works_again).
    pub async fn saving_fails_for(&self, note: impl Display) {
        self.sql(
            "CREATE FUNCTION notes.refuse_save() RETURNS trigger LANGUAGE plpgsql \
             AS $$ BEGIN RAISE EXCEPTION 'saving this note fails'; END $$"
                .into(),
        )
        .await;
        self.sql(format!(
            "CREATE TRIGGER refuse_save BEFORE UPDATE ON notes.note FOR EACH ROW \
             WHEN (OLD.id = '{note}') EXECUTE FUNCTION notes.refuse_save()"
        ))
        .await;
    }

    pub async fn saving_works_again(&self) {
        self.sql("DROP TRIGGER refuse_save ON notes.note".into())
            .await;
    }

    async fn sql(&self, statement: String) {
        sqlx::query(AssertSqlSafe(statement))
            .execute(&self.ctx.pool())
            .await
            .expect("inject the fault");
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
