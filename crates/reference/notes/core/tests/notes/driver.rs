use app_authz::{Authorizer, Rows};
use app_db::{Clause, Columns, Worker};
use app_domain::{Clock, FixedClock, seed_time};
use app_error::AppResult;
use app_events::{Event, EventPayload, NoContext, PayloadOf};
use app_module::Context;
use app_persistence::{PendingEvent, Version};
use app_test_db::Template;
use app_web::testing::{Answered, Call, Page, Refused, TestApi};
use app_web::{Accept, JwtVerifier, json_newtype};
use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use notes::{Note, NotesEndpoints, seed};
use notes_api::events::v1 as notes_events;
use poem::EndpointExt;
use serde_json::json;
use sqlx::{AssertSqlSafe, PgPool, QueryBuilder};
use std::fmt::Display;
use std::sync::{Arc, Mutex};
use users_api::UserDirectory;
use users_api::events::v1::{self as users_events, UserTopic};
use uuid::Uuid;

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
    Stale,
    /// The `Idempotency-Key` was sent before with a different request.
    KeyReused,
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
            "idempotency_key_reused" => Refusal::KeyReused,
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
            "items": n.items().iter().map(|item| json!({
                "id": item.id().to_string(),
                "text": item.text(),
            })).collect::<Vec<_>>(),
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
        items: note
            .items()
            .iter()
            .map(|item| notes_events::Item {
                id: item.id().as_uuid(),
                text: item.text().to_string(),
            })
            .collect(),
        created_at: Some(note.created_at()),
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
        Ok([users_api::seed::USER_ID_ALICE, users_api::seed::USER_ID_BOB].contains(&id))
    }
}

/// The role every signed-in user has (ADR 0040), which notes' default grants name.
pub const USER: &str = "user";

pub struct Notes {
    api: TestApi,
    ctx: Context,
    clock: Arc<FixedClock>,
    worker: Worker,
    seeded_through: i64,
    verifier: JwtVerifier,
    /// The token later requests present, if any.
    caller: Mutex<Option<String>>,
}

impl Notes {
    pub async fn seeded(pool: PgPool) -> Self {
        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let policies = notes::policies().build().expect("notes' policies load");
        let authorizer = Authorizer::from_policies(&[policies]).expect("notes' policies compile");
        let ctx = Context::new(pool, clock.clone()).with_authorizer(Arc::new(authorizer));
        let handlers = notes::handlers(&ctx, Arc::new(KnownUsers));
        app_db::register_subscriptions(&ctx.pool(), clock.as_ref(), &handlers)
            .await
            .expect("register notes' subscriptions");
        let worker = ctx.worker(handlers);
        let seeded_through = worker.outbox_end().await.expect("read the outbox");
        let verifier = JwtVerifier::new("test-secret");
        let api = TestApi::new(NotesEndpoints, ctx.idempotency(), |app| {
            app.accept(notes::provider(&ctx, Arc::new(KnownUsers)))
                .data(verifier.clone())
                .data(ctx.authorizer())
        });
        let notes = Self {
            api,
            ctx,
            clock,
            worker,
            seeded_through,
            verifier,
            caller: Mutex::new(None),
        };
        notes.sign_in_as(users_api::seed::USER_ID_ALICE, &[USER]);
        notes
    }

    /// Later requests are `user`'s, with a real token, holding `user` and
    /// `roles`, assigned in the rule set in force as `access` would. Every test
    /// starts signed in as Alice, a `user`. Roles are the subject's, not the
    /// token's: a later `by(..)` verb for the same subject resets them to `user`.
    pub fn sign_in_as(&self, user: impl Display, roles: &[&str]) {
        let user = user.to_string();
        self.ctx.authorizer().assign_in_memory(&user, roles);
        *self.caller.lock().unwrap() = Some(self.token(user));
    }

    /// Later requests present no token.
    pub fn sign_out(&self) {
        *self.caller.lock().unwrap() = None;
    }

    fn token(&self, user: impl Display) -> String {
        self.verifier
            .issue(user.to_string(), 60)
            .expect("issue token")
    }

    fn signed<'c>(&self, call: Call<'c>) -> Call<'c> {
        match self.caller.lock().unwrap().as_ref() {
            Some(token) => call.bearer(token),
            None => call,
        }
    }

    /// As `author`, holding only `user`, signed in for this request alone.
    fn by<'c>(&self, author: impl Display, call: Call<'c>) -> Call<'c> {
        let author = author.to_string();
        self.ctx.authorizer().assign_in_memory(&author, &[]);
        call.bearer(self.token(author))
    }

    /// The rule set the endpoints are served under.
    pub fn authorizer(&self) -> Arc<Authorizer> {
        self.ctx.authorizer()
    }

    /// Stores `notes` as they are, beside the seeds.
    pub async fn stored(&self, notes: Vec<Note>) {
        seed::save_missing(&self.ctx.pool(), notes)
            .await
            .expect("store the notes");
    }

    /// The ids of the stored notes `rows` holds, queried as the repository's list scopes it,
    /// through `C`'s columns.
    pub async fn ids_where<C: Columns>(&self, rows: Rows) -> Vec<Uuid> {
        let mut sql = QueryBuilder::new("SELECT id FROM notes.note");
        app_db::push_scope::<C>(&mut sql, Clause::Where, &rows);
        sql.build_query_scalar()
            .fetch_all(&self.ctx.pool())
            .await
            .expect("query the notes")
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    pub fn later(&self, by: TimeDelta) {
        self.clock.advance(by);
    }

    /// The users module announcing a seeded user's deletion, as it would after
    /// `DELETE /users/me` at their seeded version: the event is the contract, so
    /// it stands in for running users.
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
                    Version::FIRST.next(),
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

    /// Every `E` notes has announced to other modules, oldest first, with the
    /// version of the note each announces.
    pub async fn announced<E: Event>(&self) -> Vec<(Version, PayloadOf<E>)> {
        self.worker
            .published_after::<E>(self.seeded_through)
            .await
            .expect("read the outbox")
            .into_iter()
            .map(|event| (Version::from_i64(event.entity_version), event.data))
            .collect()
    }

    /// Fault injection: every event announced so far is delivered again, as
    /// at-least-once delivery allows.
    pub async fn everything_is_delivered_again(&self) {
        self.sql("UPDATE messaging.outbox SET dispatched_at = NULL".into())
            .await;
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
        self.signed(self.api.get(format!("/api/notes/{id}")))
            .send()
            .await
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

    /// As whoever is signed in, which a listing by a `user` never refuses.
    pub async fn try_list(&self) -> Outcome<NotesPage> {
        self.list_with(&[]).await
    }

    async fn list_with(&self, query: &[(&str, String)]) -> Outcome<NotesPage> {
        self.signed(self.api.get("/api/notes").queries(query))
            .send()
            .await
    }

    /// Signed in as `author`: the author is the caller.
    pub async fn create(&self, author: impl Display, title: &str, body: &str) -> Outcome<NoteJson> {
        let request = json!({ "title": title, "body": body });
        self.by(author, self.api.post("/api/notes"))
            .json(&request)
            .send()
            .await
    }

    /// As whoever is signed in, the body naming `claimed` as its author, as
    /// the request once did.
    pub async fn create_claiming_to_be(
        &self,
        claimed: impl Display,
        title: &str,
    ) -> Outcome<NoteJson> {
        let request = json!({ "authorId": claimed.to_string(), "title": title, "body": "" });
        self.signed(self.api.post("/api/notes"))
            .json(&request)
            .send()
            .await
    }

    /// The body omits `title` entirely, rather than sending it blank.
    pub async fn create_without_a_title(&self, author: impl Display) -> Outcome<NoteJson> {
        let request = json!({ "body": "" });
        self.by(author, self.api.post("/api/notes"))
            .json(&request)
            .send()
            .await
    }

    pub async fn edit(
        &self,
        id: impl Display,
        version: i64,
        title: &str,
        body: &str,
    ) -> Outcome<NoteJson> {
        let request = json!({ "title": title, "body": body });
        self.signed(self.api.patch(format!("/api/notes/{id}")))
            .if_match(version)
            .json(&request)
            .send()
            .await
    }

    pub async fn edit_without_precondition(
        &self,
        id: impl Display,
        title: &str,
        body: &str,
    ) -> Outcome<NoteJson> {
        let request = json!({ "title": title, "body": body });
        self.signed(self.api.patch(format!("/api/notes/{id}")))
            .json(&request)
            .send()
            .await
    }

    pub async fn delete(&self, id: impl Display, version: i64) -> Outcome<()> {
        self.signed(self.api.delete(format!("/api/notes/{id}")))
            .if_match(version)
            .send_empty()
            .await
    }

    /// The same verbs, each sent with `key` as its `Idempotency-Key`, answered
    /// whole so a retry's answer compares with the original's.
    pub fn with_idempotency_key(&self, key: &'static str) -> Keyed<'_> {
        Keyed { notes: self, key }
    }

    pub async fn add_item(
        &self,
        note: impl Display,
        version: i64,
        text: &str,
    ) -> Outcome<NoteJson> {
        self.signed(self.api.post(format!("/api/notes/{note}/items")))
            .if_match(version)
            .json(&json!({ "text": text }))
            .send()
            .await
    }

    pub async fn remove_item(
        &self,
        note: impl Display,
        version: i64,
        item: impl Display,
    ) -> Outcome<NoteJson> {
        self.signed(self.api.delete(format!("/api/notes/{note}/items/{item}")))
            .if_match(version)
            .send()
            .await
    }

    pub async fn delete_without_precondition(&self, id: impl Display) -> Outcome<()> {
        self.signed(self.api.delete(format!("/api/notes/{id}")))
            .send_empty()
            .await
    }
}

pub struct Keyed<'a> {
    notes: &'a Notes,
    key: &'static str,
}

impl Keyed<'_> {
    pub async fn create(&self, author: impl Display, title: &str, body: &str) -> Answered {
        let request = json!({ "title": title, "body": body });
        self.notes
            .by(author, self.notes.api.post("/api/notes"))
            .idempotency_key(self.key)
            .json(&request)
            .answered()
            .await
    }

    pub async fn edit(&self, id: impl Display, version: i64, title: &str, body: &str) -> Answered {
        let request = json!({ "title": title, "body": body });
        self.notes
            .signed(self.notes.api.patch(format!("/api/notes/{id}")))
            .idempotency_key(self.key)
            .if_match(version)
            .json(&request)
            .answered()
            .await
    }

    pub async fn delete(&self, id: impl Display, version: i64) -> Answered {
        self.notes
            .signed(self.notes.api.delete(format!("/api/notes/{id}")))
            .idempotency_key(self.key)
            .if_match(version)
            .answered()
            .await
    }
}

/// Every test's database starts as a copy of this: migrated and seeded.
pub const TEMPLATE: Template = Template::new("notes", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &[notes::migrations()])
            .await
            .expect("migrate notes");
        seed::standard(&pool).await.expect("seed notes");
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
