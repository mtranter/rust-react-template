//! The only place in these tests that knows the transport (ADR 0004). It stubs
//! the providers as a subscriber's driver does: it publishes their events
//! itself, built from their seed data as published, and runs the worker that
//! keeps note-search's copy (ADR 0036).

use app_authz::Authorizer;
use app_db::Worker;
use app_domain::{FixedClock, seed_time};
use app_module::Context;
use app_persistence::{Handlers, PendingEvent, Version};
use app_test_db::Template;
use app_web::testing::{Page, Refused, TestApi};
use app_web::{Accept, JwtVerifier, json_newtype};
use note_search::NoteSearchEndpoints;
use notes_api::events::v1 as notes_events;
use poem::EndpointExt;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use users_api::events::v1 as users_events;

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The request couldn't be parsed.
    Malformed,
    /// Signed in, but the policies don't allow it.
    Forbidden,
    /// The copy hasn't caught up with what the providers had published yet.
    Unavailable,
}

impl From<Refused> for Refusal {
    fn from(refused: Refused) -> Self {
        match refused.code.as_str() {
            "malformed_request" => Refusal::Malformed,
            "forbidden" => Refusal::Forbidden,
            "unavailable" => Refusal::Unavailable,
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
    ctx: Context,
    worker: Worker,
    verifier: JwtVerifier,
}

impl NoteSearch {
    /// Added to an app whose providers had already published every seeded
    /// user and note, so its copy starts from their backfill; caught up.
    pub async fn seeded(pool: PgPool) -> Self {
        let search = Self::added_after_the_seeds(pool).await;
        search.run_pending_events().await;
        search
    }

    /// As [`seeded`](Self::seeded), before its worker has run: its copy hasn't
    /// caught up.
    pub async fn added_after_the_seeds(pool: PgPool) -> Self {
        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let policies = note_search::policies()
            .build()
            .expect("note-search's policies load");
        let authorizer =
            Authorizer::from_policies(&[policies]).expect("note-search's policies compile");
        let ctx = Context::new(pool, clock.clone()).with_authorizer(Arc::new(authorizer));
        let verifier = JwtVerifier::new("test-secret");
        let search = Self {
            api: TestApi::new(NoteSearchEndpoints, ctx.idempotency(), |app| {
                app.accept(note_search::provider(&ctx))
                    .data(verifier.clone())
                    .data(ctx.authorizer())
            }),
            worker: ctx.worker(note_search::handlers(&ctx)),
            ctx,
            verifier,
        };
        for user in [users_api::seed::alice(), users_api::seed::bob()] {
            search
                .publish(user_event(&user, 1, users_events::UserEvent::Registered))
                .await;
        }
        for note in [
            notes_api::seed::alices_note(),
            notes_api::seed::bobs_note(),
            notes_api::seed::orphaned_note(),
        ] {
            search
                .publish(note_event(&note, 1, notes_events::NoteEvent::Created))
                .await;
        }
        search
            .ctx
            .worker(Handlers::new())
            .relay()
            .await
            .expect("relay the providers' events");
        app_db::register_subscriptions(
            &search.ctx.pool(),
            search.ctx.clock().as_ref(),
            &note_search::handlers(&search.ctx),
        )
        .await
        .expect("register note-search's subscriptions");
        search
    }

    /// Delivers the events published so far, as the worker would.
    pub async fn run_pending_events(&self) {
        self.worker
            .run_until_idle()
            .await
            .expect("the worker reaches the database");
    }

    /// The users module announcing that a seeded user now goes by `name`.
    pub async fn user_renamed(&self, user: users_api::v1::User, name: &str) {
        let mut user = user;
        user.display_name = name.into();
        self.publish(user_event(
            &user,
            2,
            users_events::UserEvent::DisplayNameChanged,
        ))
        .await;
        self.run_pending_events().await;
    }

    pub async fn user_deleted(&self, user: users_api::v1::User) {
        self.publish(user_event(&user, 2, users_events::UserEvent::Deleted))
            .await;
        self.run_pending_events().await;
    }

    /// The notes module announcing that a seeded note is now titled `title`.
    pub async fn note_retitled(&self, note: notes_api::v1::Note, title: &str) {
        let mut note = note;
        note.title = title.into();
        self.publish(note_event(&note, 2, notes_events::NoteEvent::Edited))
            .await;
        self.run_pending_events().await;
    }

    pub async fn note_deleted(&self, note: notes_api::v1::Note) {
        self.publish(note_event(&note, 2, notes_events::NoteEvent::Deleted))
            .await;
        self.run_pending_events().await;
    }

    async fn publish(&self, event: PendingEvent) {
        let mut tx = self.ctx.transactions().begin().await.expect("begin");
        self.ctx
            .publisher()
            .publish(&mut tx, event)
            .await
            .expect("publish");
        tx.commit().await.expect("commit");
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

    /// As Alice, a `user`.
    pub async fn search(&self, query: &[(&str, String)]) -> Outcome<Found> {
        let token = self
            .verifier
            .issue(users_api::seed::USER_ID_ALICE.to_string(), 60)
            .expect("issue token");
        self.api
            .get("/api/note-search")
            .bearer(token)
            .queries(query)
            .send()
            .await
    }
}

/// `user` as the users module announces it, at `version`.
fn user_event(
    user: &users_api::v1::User,
    version: i64,
    event: users_events::UserEvent,
) -> PendingEvent {
    PendingEvent::on::<users_events::UserTopic>(
        Version::from_i64(version),
        &users_events::User {
            id: user.id,
            email: user.email.clone(),
            display_name: user.display_name.clone(),
        },
        event,
    )
}

/// `note` as the notes module announces it, at `version`.
fn note_event(
    note: &notes_api::v1::Note,
    version: i64,
    event: notes_events::NoteEvent,
) -> PendingEvent {
    PendingEvent::on::<notes_events::NoteTopic>(
        Version::from_i64(version),
        &notes_events::Note {
            id: note.id,
            author_id: note.author_id,
            title: note.title.clone(),
            body: String::new(),
            archived_at: None,
            items: Vec::new(),
            created_at: Some(note.created_at),
        },
        event,
    )
}

/// Every test's database starts as a copy of this: note-search migrated, its
/// copy empty.
pub const TEMPLATE: Template = Template::new("note_search", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &[note_search::migrations()])
            .await
            .expect("migrate note-search");
    })
});
