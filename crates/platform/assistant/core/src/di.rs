// Naming the pool to wire the repository is this module's DI seam (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::{AssistantService, ChatModel, Settings};
use crate::db::{PgConversationRepository, migrations};
use crate::domain::{Assistant, Conversation};
use crate::providers::{AnthropicAuth, RigModel};
use crate::web::OPERATIONS;
use app_authz::{Policies, PoliciesBuilder, policy_file};
use app_module::{Context, Module};
use app_web::Provide;
use assistant_api::events::v1::ConversationTopic;
use std::sync::Arc;

pub struct AssistantProvider(AssistantService);

impl Provide for AssistantProvider {
    type Data = AssistantService;

    fn build(self) -> AssistantService {
        self.0
    }
}

/// `model` is `None` while none is configured, and the assistant is off; tests pass a fake.
pub fn provider(
    ctx: &Context,
    model: Option<Arc<dyn ChatModel>>,
    settings: Settings,
) -> AssistantProvider {
    AssistantProvider(AssistantService::new(
        model,
        settings,
        Arc::new(PgConversationRepository::new(ctx.pool())),
        ctx.publisher(),
        ctx.transactions(),
        ctx.clock(),
        ctx.authorizer(),
    ))
}

/// The assistant's `assistant` resource, whose `chat` every `user` is granted by
/// default (ADR 0040), and its `conversation` resource, scoped to each one's `owner`.
pub fn policies() -> PoliciesBuilder {
    Policies::builder("assistant")
        .resource::<Assistant>(policy_file!("policies/assistant.yaml"))
        .resource::<Conversation>(policy_file!("policies/conversation.yaml"))
        .derived_roles(policy_file!("policies/derived_roles.yaml"))
        .default_grants(policy_file!("policies/default_grants.yaml"))
        .labels(policy_file!("policies/labels.yaml"))
}

/// `settings` are read when the API is built, so `dump-spec` and `migrate` need none. With a
/// model configured, it is Anthropic's with an Anthropic credential in the environment, else
/// Bedrock's.
pub fn module(settings: fn() -> Settings) -> Module {
    module_with(settings, configured)
}

/// [`module`] with the model `model` makes of the settings, for a composition that must not call
/// a provider, such as the journeys'.
pub fn module_with(
    settings: fn() -> Settings,
    model: fn(&Settings) -> Option<Arc<dyn ChatModel>>,
) -> Module {
    let module = Module::new(move |ctx| {
        let settings = settings();
        provider(ctx, model(&settings), settings)
    })
    .with_policies(policies)
    .performing(OPERATIONS)
    .publishes::<ConversationTopic>()
    .with_migrations(migrations());
    #[cfg(feature = "seed")]
    let module = module.with_seed(|ctx| async move { crate::seed::standard(&ctx.pool()).await });
    module
}

fn configured(settings: &Settings) -> Option<Arc<dyn ChatModel>> {
    let model = settings.model.as_deref()?;
    Some(Arc::new(match AnthropicAuth::from_env() {
        Some(auth) => RigModel::anthropic(model, auth),
        None => RigModel::bedrock(model),
    }))
}
