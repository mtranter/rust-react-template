use app_authz::{Authorizer, Grant, Rows, RuleSet};
use app_db::{Clause, Columns, Worker};
use app_domain::{FixedClock, seed_time};
use app_error::AppError;
use app_events::{Event, EventPayload, NoContext, PayloadOf};
use app_module::Context;
use app_persistence::{Handlers, Version};
use app_test_db::Template;
use app_web::mcp::{RestOnly, Surface, Toolbox};
use app_web::testing::{Call, Page, Refused, TestApi};
use app_web::{Accept, ApiResult, JwtAuth, JwtVerifier, json_newtype, render_error};
use assistant::{
    AssistantEndpoints, ChatModel, Content, Conversation, ModelError, ModelReply, ModelRequest,
    ModelToolCall, Outcome as ToolOutcome, Settings, ToolCall, ToolCallStatus, seed,
};
use assistant_api::events::v1 as events;
use async_trait::async_trait;
use poem::endpoint::BoxEndpoint;
use poem::web::Data;
use poem::{EndpointExt, Route};
use poem_openapi::param::Path;
use poem_openapi::payload::Json as PoemJson;
use poem_openapi::{OpenApi, OpenApiService};
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
    /// The conversation's state doesn't allow it now.
    Conflict,
    /// The assistant is off.
    Unavailable,
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
            "conflict" => Refusal::Conflict,
            "unavailable" => Refusal::Unavailable,
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
                    Content::Error(text) => json!({
                        "kind": "error", "id": id, "postedAt": posted_at, "text": text,
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

/// What the scripted model does at one step of a turn.
pub enum Step {
    Reply(ModelReply),
    Fail(String),
}

/// The model saying `text`.
pub fn says(text: &str) -> Step {
    Step::Reply(ModelReply {
        text: text.to_string(),
        output_tokens: 10,
        ..ModelReply::default()
    })
}

/// The model asking for `tool` with `arguments`.
pub fn calls(tool: &str, arguments: Value) -> Step {
    calls_all(&[(tool, arguments)])
}

/// The model asking for each of `asked` in one step.
pub fn calls_all(asked: &[(&str, Value)]) -> Step {
    Step::Reply(ModelReply {
        tool_calls: asked
            .iter()
            .enumerate()
            .map(|(n, (tool, arguments))| ModelToolCall {
                id: format!("call-{n}"),
                name: tool.to_string(),
                arguments: arguments.clone(),
            })
            .collect(),
        output_tokens: 10,
        ..ModelReply::default()
    })
}

/// Stands in for Bedrock, which no test calls: takes each step of a turn from its script,
/// streaming each reply's words in two pieces, and fails once the script runs out. Keeps every
/// request it was sent.
#[derive(Default)]
pub struct ScriptedModel {
    script: Mutex<VecDeque<Step>>,
    requests: Mutex<Vec<ModelRequest>>,
}

#[async_trait]
impl ChatModel for ScriptedModel {
    async fn reply(
        &self,
        request: ModelRequest,
        text: &(dyn for<'a> Fn(&'a str) + Send + Sync),
    ) -> Result<ModelReply, ModelError> {
        self.requests.lock().unwrap().push(request);
        let step = self.script.lock().unwrap().pop_front();
        match step {
            Some(Step::Reply(reply)) => {
                let middle = reply
                    .text
                    .char_indices()
                    .nth(reply.text.chars().count() / 2);
                let (first, second) = reply.text.split_at(middle.map_or(0, |(at, _)| at));
                for piece in [first, second]
                    .into_iter()
                    .filter(|piece| !piece.is_empty())
                {
                    text(piece);
                }
                Ok(reply)
            }
            Some(Step::Fail(why)) => Err(ModelError(why)),
            None => Err(ModelError("the script has run out".to_string())),
        }
    }
}

/// The list the stand-in for the app's tools has, by this id.
pub const LIST_ID: &str = "11111111-1111-1111-1111-111111111111";

/// The app's operations, standing in for those the server composes beside the assistant: a
/// read, `get_list`, answering who read it, and a write, `create_list`, which keeps what it is
/// sent.
#[derive(Default)]
struct AppOperations;

#[derive(Clone, Default)]
struct Written(Arc<Mutex<Vec<Value>>>);

#[OpenApi]
impl AppOperations {
    /// Fetch a list by its id. Another's, or one that doesn't exist, is refused with `404`.
    #[oai(path = "/lists/:id", method = "get", operation_id = "get_list")]
    async fn get_list(&self, auth: JwtAuth, id: Path<String>) -> ApiResult<PoemJson<Value>> {
        if id.0 != LIST_ID {
            return Err(AppError::not_found("list").into());
        }
        Ok(PoemJson(json!({
            "id": LIST_ID,
            "title": "Shopping",
            "readBy": auth.0.id(),
        })))
    }

    /// Write a list, owned by the caller.
    #[oai(path = "/lists", method = "post", operation_id = "create_list")]
    async fn create_list(
        &self,
        _auth: JwtAuth,
        written: Data<&Written>,
        body: PoemJson<Value>,
    ) -> ApiResult<PoemJson<Value>> {
        written.0.0.lock().unwrap().push(body.0.clone());
        Ok(PoemJson(body.0))
    }
}

struct AppSurface;

impl Surface for AppSurface {
    type Api = AppOperations;
    const BASE: &'static str = "/api";
    const NAME: &'static str = "Test app";
    const VERSION: &'static str = "1.0";
    const INSTRUCTIONS: &'static str = "Each tool is an operation of this app's REST API.";
    const REST_ONLY: &'static [RestOnly] = &[];
}

/// The app's operations as the assistant's tools, served by their own API, which verifies
/// tokens with `verifier`.
fn app_tools(
    verifier: JwtVerifier,
    authorizer: Arc<Authorizer>,
) -> (Toolbox, Arc<BoxEndpoint<'static>>, Written) {
    let written = Written::default();
    let api: Arc<BoxEndpoint<'static>> = Arc::new(
        Route::new()
            .nest("/api", OpenApiService::new(AppOperations, "test", "1.0"))
            .catch_all_error(render_error)
            .data(verifier)
            .data(authorizer)
            .data(written.clone())
            .boxed(),
    );
    let toolbox = Toolbox::new::<AppSurface>();
    toolbox.serve(&api);
    (toolbox, api, written)
}

/// The subject every test starts signed in as, holding only `user`: the seeds' Alice.
pub const ALICE: &str = seed::SUBJECT_ALICE;
pub const BOB: &str = seed::SUBJECT_BOB;
pub const ADMIN: &str = "admin";

pub struct Assistant {
    api: TestApi,
    /// What the toolbox calls through, which it holds only weakly.
    _tools_api: Arc<BoxEndpoint<'static>>,
    written: Written,
    model: Arc<ScriptedModel>,
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
        Self::wired(pool, Tokens::Valid, |ctx, app, _| module.provide(ctx, app)).await
    }

    /// With a scripted model in Bedrock's place, and the default limits.
    pub async fn on(pool: PgPool) -> Self {
        Self::limited(pool, Settings::default()).await
    }

    /// With a scripted model, and the limits `settings` set.
    pub async fn limited(pool: PgPool, settings: Settings) -> Self {
        Self::wired(pool, Tokens::Valid, |ctx, app, model| {
            app.accept(assistant::provider(ctx, Some(model), settings))
                .boxed()
        })
        .await
    }

    /// With a scripted model, and the app's operations refusing every token as not signed in,
    /// as they do once it expires.
    pub async fn signed_out_of_the_app(pool: PgPool) -> Self {
        Self::wired(pool, Tokens::RefusedByTheApp, |ctx, app, model| {
            app.accept(assistant::provider(ctx, Some(model), Settings::default()))
                .boxed()
        })
        .await
    }

    async fn wired(
        pool: PgPool,
        tokens: Tokens,
        provide: impl FnOnce(&Context, BoxEndpoint<'static>, Arc<dyn ChatModel>) -> BoxEndpoint<'static>,
    ) -> Self {
        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let authorizer = Authorizer::from_policies(&[policies()]).expect("policies compile");
        let ctx = Context::new(pool, clock.clone()).with_authorizer(Arc::new(authorizer));
        let worker = ctx.worker(Handlers::new());
        let seeded_through = worker.outbox_end().await.expect("read the outbox");
        let verifier = JwtVerifier::new("test-secret");
        let apps_verifier = match tokens {
            Tokens::Valid => verifier.clone(),
            Tokens::RefusedByTheApp => JwtVerifier::new("another-secret"),
        };
        let (toolbox, tools_api, written) = app_tools(apps_verifier, ctx.authorizer());
        let model = Arc::new(ScriptedModel::default());
        let api = TestApi::new(AssistantEndpoints, ctx.idempotency(), |app| {
            provide(&ctx, app, model.clone())
                .data(verifier.clone())
                .data(ctx.authorizer())
                .data(toolbox)
                .data(ctx.draining())
        });
        let assistant = Self {
            api,
            _tools_api: tools_api,
            written,
            model,
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

    /// The model's next steps, in order.
    pub fn scripted(&self, steps: impl IntoIterator<Item = Step>) {
        self.model.script.lock().unwrap().extend(steps);
    }

    /// Every request the model was sent, oldest first.
    pub fn model_requests(&self) -> Vec<ModelRequest> {
        self.model.requests.lock().unwrap().clone()
    }

    /// What the app's write operation was sent, oldest first.
    pub fn written(&self) -> Vec<Value> {
        self.written.0.lock().unwrap().clone()
    }

    /// As the server does when it begins to shut down.
    pub fn begin_draining(&self) {
        self.ctx.draining().begin();
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

    /// Says `text` in conversation `id` at `version`: the turn's events, once it has ended.
    pub async fn send(&self, id: impl Display, version: i64, text: &str) -> Outcome<Vec<Value>> {
        self.signed(self.message(id, text))
            .if_match(version)
            .stream()
            .await
    }

    pub async fn send_without_precondition(
        &self,
        id: impl Display,
        text: &str,
    ) -> Outcome<Vec<Value>> {
        self.signed(self.message(id, text)).stream().await
    }

    /// Says `text`, then hangs up as soon as the turn's stream has begun, as a closed tab does.
    pub async fn send_and_hang_up(&self, id: impl Display, version: i64, text: &str) {
        self.signed(self.message(id, text))
            .if_match(version)
            .hang_up()
            .await;
    }

    fn message(&self, id: impl Display, text: &str) -> Call<'_> {
        self.api
            .post(format!("/api/assistant/conversations/{id}/messages"))
            .json(&json!({ "text": text }))
    }

    /// The conversation once it holds `messages` messages, as a turn still running stores
    /// them; panics if it never does.
    pub async fn eventually(&self, id: impl Display, messages: usize) -> ConversationJson {
        let id = id.to_string();
        for _ in 0..200 {
            let conversation = self.get(&id).await.expect("the conversation reads");
            if conversation["messages"].as_array().map_or(0, Vec::len) >= messages {
                return conversation;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("the conversation never held {messages} messages");
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

/// Whether the app's operations accept the tokens the assistant's do.
enum Tokens {
    Valid,
    RefusedByTheApp,
}

/// Each event of `events` of `kind`.
pub fn of_kind<'e>(events: &'e [Value], kind: &str) -> Vec<&'e Value> {
    events
        .iter()
        .filter(|event| event["kind"] == kind)
        .collect()
}

/// The words the turn streamed, joined.
pub fn streamed_text(events: &[Value]) -> String {
    of_kind(events, "text")
        .iter()
        .map(|event| event["text"].as_str().unwrap_or_default())
        .collect()
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
