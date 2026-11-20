use app_authz::{Authorizer, Grant, Rows, RuleSet};
use app_db::{Clause, Columns, Worker};
use app_domain::{FixedClock, seed_time};
use app_events::{Event, EventPayload, NoContext, PayloadOf};
use app_module::Context;
use app_persistence::{Handlers, Version};
use app_test_db::Template;
use app_web::testing::{Call, Page, Refused, TestApi};
use app_web::{Accept, JwtVerifier, json_newtype};
use assistant::{
    AssistantEndpoints, ChatModel, Content, Conversation, ModelError, ModelReply, ModelRequest,
    Outcome as ToolOutcome, Settings, ToolCall, ToolCallStatus, seed,
};
use assistant_api::events::v1 as events;
use async_trait::async_trait;
use poem::EndpointExt;
use poem::endpoint::BoxEndpoint;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgPool, QueryBuilder};
use std::collections::VecDeque;
use std::fmt::Display;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The request couldn't be parsed.
    Malformed,
    /// A rule was broken.
    Invalid,
    NotSignedIn,
    /// Signed in, but the policies don't allow it.
    Forbidden,
    NotFound,
    PreconditionRequired,
    Stale,
}

impl From<Refused> for Refusal {
    fn from(refused: Refused) -> Self {
        match refused.code.as_str() {
            "malformed_request" => Refusal::Malformed,
            "validation_error" => Refusal::Invalid,
            "unauthorized" => Refusal::NotSignedIn,
            "forbidden" => Refusal::Forbidden,
            "not_found" => Refusal::NotFound,
            "precondition_required" => Refusal::PreconditionRequired,
            "precondition_failed" => Refusal::Stale,
            other => panic!("unexpected error code {other}"),
        }
    }
}

pub type Outcome<T> = Result<T, Refusal>;

#[derive(Debug, PartialEq, Eq, Deserialize)]
pub struct Status {
    pub on: bool,
}

json_newtype!(pub ConversationJson);

impl From<Conversation> for ConversationJson {
    /// Seeded and freshly started conversations are always at version `1`.
    fn from(c: Conversation) -> Self {
        Self(json!({
            "id": c.id().to_string(),
            "title": c.title(),
            "createdAt": c.created_at().to_rfc3339(),
            "messages": c.messages().iter().map(|message| {
                let id = message.id().to_string();
                let posted_at = message.posted_at().to_rfc3339();
                match message.content() {
                    Content::User(text) => json!({
                        "kind": "user", "id": id, "postedAt": posted_at, "text": text,
                    }),
                    Content::Assistant(text) => json!({
                        "kind": "assistant", "id": id, "postedAt": posted_at, "text": text,
                    }),
                    Content::ToolCalls(calls) => json!({
                        "kind": "toolCalls",
                        "id": id,
                        "postedAt": posted_at,
                        "toolCalls": calls.iter().map(tool_call).collect::<Vec<_>>(),
                    }),
                }
            }).collect::<Vec<_>>(),
            "version": "1",
        }))
    }
}

fn tool_call(call: &ToolCall) -> Value {
    let mut json = json!({
        "id": call.id().to_string(),
        "tool": call.tool(),
        "arguments": call.arguments(),
        "status": match call.status() {
            ToolCallStatus::Ran => "ran",
            ToolCallStatus::AwaitingApproval => "awaiting_approval",
            ToolCallStatus::Approved => "approved",
            ToolCallStatus::Rejected => "rejected",
        },
    });
    match call.outcome() {
        Some(ToolOutcome::Ok(body)) => json["result"] = body.clone(),
        Some(ToolOutcome::Error(body)) => json["error"] = body.clone(),
        None => {}
    }
    json
}

json_newtype!(pub SummaryJson);

pub type ConversationsPage = Page<SummaryJson>;

/// What a conversation event carries about `conversation`.
pub fn announced(conversation: &Conversation) -> EventPayload<events::Conversation, NoContext> {
    EventPayload::new(events::Conversation {
        id: conversation.id().as_uuid(),
        owner_id: conversation.owner_id().to_string(),
        title: conversation.title().to_string(),
        created_at: conversation.created_at(),
    })
}

/// Stands in for Bedrock, which no test calls: answers each request with the next of the
/// replies it was given, and fails once they run out.
#[derive(Default)]
pub struct ScriptedModel {
    replies: Mutex<VecDeque<ModelReply>>,
}

#[async_trait]
impl ChatModel for ScriptedModel {
    async fn reply(&self, _request: ModelRequest) -> Result<ModelReply, ModelError> {
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ModelError("the script has run out".to_string()))
    }
}

/// The subject every test starts signed in as, holding only `user`: the seeds' Alice.
pub const ALICE: &str = seed::SUBJECT_ALICE;
pub const BOB: &str = seed::SUBJECT_BOB;
pub const ADMIN: &str = "admin";

pub struct Assistant {
    api: TestApi,
    ctx: Context,
    clock: Arc<FixedClock>,
    worker: Worker,
    seeded_through: i64,
    verifier: JwtVerifier,
    /// The token later requests present, if any.
    caller: Mutex<Option<String>>,
}

impl Assistant {
    /// Registered as the server registers it, with `settings` for its configuration: with a
    /// model, its Bedrock is built but never called.
    pub async fn configured(pool: PgPool, settings: fn() -> Settings) -> Self {
        let module = assistant::module(settings);
        Self::wired(pool, |ctx, app| module.provide(ctx, app)).await
    }

    /// With a scripted model in Bedrock's place.
    pub async fn on(pool: PgPool) -> Self {
        let model: Arc<dyn ChatModel> = Arc::new(ScriptedModel::default());
        Self::wired(pool, |ctx, app| {
            app.accept(assistant::provider(ctx, Some(model))).boxed()
        })
        .await
    }

    async fn wired(
        pool: PgPool,
        provide: impl FnOnce(&Context, BoxEndpoint<'static>) -> BoxEndpoint<'static>,
    ) -> Self {
        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let authorizer = Authorizer::from_policies(&[policies()]).expect("policies compile");
        let ctx = Context::new(pool, clock.clone()).with_authorizer(Arc::new(authorizer));
        let worker = ctx.worker(Handlers::new());
        let seeded_through = worker.outbox_end().await.expect("read the outbox");
        let verifier = JwtVerifier::new("test-secret");
        let api = TestApi::new(AssistantEndpoints, ctx.idempotency(), |app| {
            provide(&ctx, app)
                .data(verifier.clone())
                .data(ctx.authorizer())
        });
        let assistant = Self {
            api,
            ctx,
            clock,
            worker,
            seeded_through,
            verifier,
            caller: Mutex::new(None),
        };
        assistant.sign_in_as(ALICE, &[]);
        assistant
    }

    /// Later requests are `subject`'s, holding `user` and `roles`, with a real token.
    pub fn sign_in_as(&self, subject: &str, roles: &[&str]) {
        self.ctx.authorizer().assign_in_memory(subject, roles);
        let token = self.verifier.issue(subject, 60).expect("issue token");
        *self.caller.lock().unwrap() = Some(token);
    }

    /// Later requests present no token.
    pub fn sign_out(&self) {
        *self.caller.lock().unwrap() = None;
    }

    /// As an admin revoking `user`'s `chat` would leave the rules, through `access`.
    pub fn revoke_chat(&self) {
        let policies = policies();
        let grants: Vec<Grant> = policies
            .default_grants()
            .iter()
            .filter(|grant| !(grant.resource == "assistant" && grant.action == "chat"))
            .cloned()
            .collect();
        let rules = RuleSet::compile(&[policies], &grants).expect("rules compile");
        self.ctx.authorizer().replace(rules.rule_set);
    }

    /// The rule set the endpoints are served under.
    pub fn authorizer(&self) -> Arc<Authorizer> {
        self.ctx.authorizer()
    }

    pub fn now(&self) -> chrono::DateTime<chrono::Utc> {
        use app_domain::Clock;
        self.clock.now()
    }

    /// Stores `conversations` as they are, beside the seeds.
    pub async fn stored(&self, conversations: Vec<Conversation>) {
        seed::save_missing(&self.ctx.pool(), conversations)
            .await
            .expect("store the conversations");
    }

    /// The ids of the stored conversations `rows` holds, queried as the repository's list
    /// scopes it, through `C`'s columns.
    pub async fn ids_where<C: Columns>(&self, rows: Rows) -> Vec<Uuid> {
        let mut sql = QueryBuilder::new("SELECT id FROM assistant.conversation");
        app_db::push_scope::<C>(&mut sql, Clause::Where, &rows);
        sql.build_query_scalar()
            .fetch_all(&self.ctx.pool())
            .await
            .expect("query the conversations")
    }

    /// Every `E` the assistant has announced, oldest first, with the version of the
    /// conversation each announces.
    pub async fn announced<E: Event>(&self) -> Vec<(Version, PayloadOf<E>)> {
        self.worker
            .published_after::<E>(self.seeded_through)
            .await
            .expect("read the outbox")
            .into_iter()
            .map(|event| (Version::from_i64(event.entity_version), event.data))
            .collect()
    }

    fn signed<'c>(&self, call: Call<'c>) -> Call<'c> {
        match self.caller.lock().unwrap().as_ref() {
            Some(token) => call.bearer(token),
            None => call,
        }
    }

    pub async fn status(&self) -> Outcome<Status> {
        self.signed(self.api.get("/api/assistant/status"))
            .send()
            .await
    }

    pub async fn start(&self) -> Outcome<ConversationJson> {
        self.signed(self.api.post("/api/assistant/conversations"))
            .send()
            .await
    }

    pub async fn get(&self, id: impl Display) -> Outcome<ConversationJson> {
        self.signed(self.api.get(format!("/api/assistant/conversations/{id}")))
            .send()
            .await
    }

    pub async fn list(&self) -> Outcome<ConversationsPage> {
        self.list_with(&[]).await
    }

    pub async fn list_page(&self, limit: i64, offset: i64) -> Outcome<ConversationsPage> {
        self.list_with(&[("limit", limit.to_string()), ("offset", offset.to_string())])
            .await
    }

    /// `sort` keys as the API names them, `-` prefix for descending.
    pub async fn list_sorted(&self, sort: &[&str]) -> Outcome<ConversationsPage> {
        let query: Vec<_> = sort.iter().map(|key| ("sort", key.to_string())).collect();
        self.list_with(&query).await
    }

    async fn list_with(&self, query: &[(&str, String)]) -> Outcome<ConversationsPage> {
        self.signed(self.api.get("/api/assistant/conversations").queries(query))
            .send()
            .await
    }

    pub async fn delete(&self, id: impl Display, version: i64) -> Outcome<()> {
        self.signed(
            self.api
                .delete(format!("/api/assistant/conversations/{id}")),
        )
        .if_match(version)
        .send_empty()
        .await
    }

    pub async fn delete_without_precondition(&self, id: impl Display) -> Outcome<()> {
        self.signed(
            self.api
                .delete(format!("/api/assistant/conversations/{id}")),
        )
        .send_empty()
        .await
    }
}

fn policies() -> app_authz::Policies {
    assistant::policies()
        .build()
        .expect("assistant's policies load")
}

/// Every test's database starts as a copy of this: migrated and seeded.
pub const TEMPLATE: Template = Template::new("assistant", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &[assistant::migrations()])
            .await
            .expect("migrate the assistant");
        seed::standard(&pool).await.expect("seed the assistant");
        // As in an environment seeded before this build.
        Worker::new(
            pool.clone(),
            Arc::new(app_domain::SystemClock),
            Default::default(),
        )
        .relay()
        .await
        .expect("relay the seeds' events");
    })
});
