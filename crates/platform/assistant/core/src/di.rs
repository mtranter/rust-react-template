use crate::application::{AssistantService, ChatModel, Settings};
use crate::bedrock::Bedrock;
use crate::domain::Assistant;
use crate::web::OPERATIONS;
use app_authz::{Policies, PoliciesBuilder, policy_file};
use app_module::{Context, Module};
use app_web::Provide;
use std::sync::Arc;

pub struct AssistantProvider(AssistantService);

impl Provide for AssistantProvider {
    type Data = AssistantService;

    fn build(self) -> AssistantService {
        self.0
    }
}

/// `model` is `None` while none is configured, and the assistant is off; tests pass a fake.
pub fn provider(ctx: &Context, model: Option<Arc<dyn ChatModel>>) -> AssistantProvider {
    AssistantProvider(AssistantService::new(model, ctx.authorizer()))
}

/// The assistant's gate and its `assistant` resource, whose `chat` every `user` is granted by
/// default (ADR 0040).
pub fn policies() -> PoliciesBuilder {
    Policies::builder("assistant")
        .resource::<Assistant>(policy_file!("policies/assistant.yaml"))
        .default_grants(policy_file!("policies/default_grants.yaml"))
}

/// `settings` are read when the API is built, so `dump-spec` and `migrate` need none.
pub fn module(settings: fn() -> Settings) -> Module {
    Module::new(move |ctx| provider(ctx, model(&settings())))
        .with_policies(policies)
        .performing(OPERATIONS)
}

fn model(settings: &Settings) -> Option<Arc<dyn ChatModel>> {
    let model = settings.model.as_deref()?;
    Some(Arc::new(Bedrock::new(model)))
}
