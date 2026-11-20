use app_authz::{Authorizer, Grant, RuleSet};
use app_domain::{FixedClock, seed_time};
use app_module::Context;
use app_test_db::Template;
use app_web::testing::{Refused, TestApi};
use app_web::{Accept, JwtVerifier};
use assistant::{AssistantEndpoints, ChatModel, ModelError, ModelReply, ModelRequest, Settings};
use async_trait::async_trait;
use poem::EndpointExt;
use poem::endpoint::BoxEndpoint;
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    NotSignedIn,
    /// Signed in, but the policies don't allow it.
    Forbidden,
}

impl From<Refused> for Refusal {
    fn from(refused: Refused) -> Self {
        match refused.code.as_str() {
            "unauthorized" => Refusal::NotSignedIn,
            "forbidden" => Refusal::Forbidden,
            other => panic!("unexpected error code {other}"),
        }
    }
}

pub type Outcome<T> = Result<T, Refusal>;

#[derive(Debug, PartialEq, Eq, Deserialize)]
pub struct Status {
    pub on: bool,
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

/// A signed-in user's subject, holding only `user`.
pub const ALICE: &str = "alice";

pub struct Assistant {
    api: TestApi,
    ctx: Context,
    verifier: JwtVerifier,
    /// The token later requests present, if any.
    caller: Mutex<Option<String>>,
}

impl Assistant {
    /// Registered as the server registers it, with `settings` for its configuration: with a
    /// model, its Bedrock is built but never called.
    pub async fn configured(pool: PgPool, settings: fn() -> Settings) -> Self {
        let module = assistant::module(settings);
        Self::wired(pool, |ctx, app| module.provide(ctx, app))
    }

    /// With a scripted model in Bedrock's place.
    pub async fn on(pool: PgPool) -> Self {
        let model: Arc<dyn ChatModel> = Arc::new(ScriptedModel::default());
        Self::wired(pool, |ctx, app| {
            app.accept(assistant::provider(ctx, Some(model))).boxed()
        })
    }

    fn wired(
        pool: PgPool,
        provide: impl FnOnce(&Context, BoxEndpoint<'static>) -> BoxEndpoint<'static>,
    ) -> Self {
        let authorizer = Authorizer::from_policies(&[policies()]).expect("policies compile");
        let ctx = Context::new(pool, Arc::new(FixedClock::new(seed_time(0))))
            .with_authorizer(Arc::new(authorizer));
        let verifier = JwtVerifier::new("test-secret");
        let api = TestApi::new(AssistantEndpoints, ctx.idempotency(), |app| {
            provide(&ctx, app)
                .data(verifier.clone())
                .data(ctx.authorizer())
        });
        let assistant = Self {
            api,
            ctx,
            verifier,
            caller: Mutex::new(None),
        };
        assistant.sign_in_as(ALICE);
        assistant
    }

    /// Later requests are `user`'s, a `user`, with a real token.
    pub fn sign_in_as(&self, user: &str) {
        self.ctx.authorizer().assign_in_memory(user, &[]);
        let token = self.verifier.issue(user, 60).expect("issue token");
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

    pub async fn status(&self) -> Outcome<Status> {
        let call = self.api.get("/api/assistant/status");
        let token = self.caller.lock().unwrap().clone();
        match token {
            Some(token) => call.bearer(token),
            None => call,
        }
        .send()
        .await
    }
}

fn policies() -> app_authz::Policies {
    assistant::policies()
        .build()
        .expect("assistant's policies load")
}

pub const TEMPLATE: Template = Template::new("assistant", |_pool| Box::pin(async {}));
