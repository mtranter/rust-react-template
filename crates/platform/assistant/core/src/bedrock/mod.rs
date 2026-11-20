//! The assistant's model on Amazon Bedrock, through rig's Converse wire. Region and credentials
//! come from the AWS SDK's default chain (`AWS_REGION`, `AWS_PROFILE`, an instance or task
//! role), loaded on the first call, so building one checks nothing.

use crate::application::{ChatModel, ModelError, ModelMessage, ModelReply, ModelRequest};
use async_trait::async_trait;
use rig_bedrock::client::BedrockRuntime;
use rig_core::DynModel;
use rig_core::completion::{CompletionRequest, Message};
use rig_core::operation::Completion;

pub struct Bedrock {
    model: DynModel<Completion>,
}

impl Bedrock {
    /// `model` is a Bedrock model or inference-profile id.
    pub fn new(model: &str) -> Self {
        Self {
            model: BedrockRuntime::from_env().completion(model).erase(),
        }
    }
}

#[async_trait]
impl ChatModel for Bedrock {
    async fn reply(&self, request: ModelRequest) -> Result<ModelReply, ModelError> {
        let mut messages = request.messages.into_iter().map(|message| match message {
            ModelMessage::User(text) => Message::user(text),
            ModelMessage::Assistant(text) => Message::assistant(text),
        });
        let prompt = messages
            .next_back()
            .ok_or_else(|| ModelError("there is no message to reply to".to_string()))?;
        let mut completion = CompletionRequest::new(prompt)
            .messages(messages)
            .preamble(request.system);
        completion.max_tokens = Some(request.max_output_tokens.into());
        let response = self
            .model
            .call(completion)
            .await
            .map_err(|error| ModelError(error.to_string()))?;
        Ok(ModelReply {
            text: response.text(),
        })
    }
}
