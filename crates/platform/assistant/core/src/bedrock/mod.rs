//! The assistant's model on Amazon Bedrock, through rig's Converse wire, streamed. Region and
//! credentials come from the AWS SDK's default chain (`AWS_REGION`, `AWS_PROFILE`, an instance
//! or task role), loaded on the first call, so building one checks nothing.

use crate::application::{
    ChatModel, ModelError, ModelMessage, ModelReply, ModelRequest, ModelToolCall,
};
use async_trait::async_trait;
use futures_util::StreamExt;
use rig_bedrock::client::BedrockRuntime;
use rig_core::DynModel;
use rig_core::completion::message::{
    AssistantContent, AssistantMessage, CallId, ToolName, ToolResult, ToolResultContent,
    UserContent,
};
use rig_core::completion::{CompletionRequest, FinishReason, Message, ToolDefinition};
use rig_core::operation::Completion;
use rig_core::streaming::{Item, StreamEvent};
use serde_json::Value;

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

fn failed(error: impl std::fmt::Display) -> ModelError {
    ModelError(error.to_string())
}

fn tool_name(name: &str) -> Result<ToolName, ModelError> {
    ToolName::new(name).map_err(failed)
}

#[async_trait]
impl ChatModel for Bedrock {
    async fn reply(
        &self,
        request: ModelRequest,
        text: &(dyn for<'a> Fn(&'a str) + Send + Sync),
    ) -> Result<ModelReply, ModelError> {
        let mut messages = conversation(request.messages)?;
        let prompt = messages
            .pop()
            .ok_or_else(|| ModelError("there is no message to reply to".to_string()))?;
        let tools = request
            .tools
            .into_iter()
            .map(|tool| {
                Ok(ToolDefinition::new(
                    tool_name(&tool.name)?,
                    tool.description,
                    tool.input_schema,
                ))
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        let completion = CompletionRequest::new(prompt)
            .messages(messages)
            .preamble(request.system)
            .tools(tools)
            .max_tokens(u64::from(request.max_output_tokens));

        let mut stream = self.model.stream(completion).map_err(failed)?;
        while let Some(item) = stream.next().await {
            if let Item::Event(StreamEvent::Text { text: piece, .. }) = item.map_err(failed)? {
                text(&piece);
            }
        }
        let response = stream.finish().await.map_err(failed)?;

        let mut reply = ModelReply {
            output_tokens: response
                .usage
                .output_tokens
                .map_or(0, |tokens| u32::try_from(tokens).unwrap_or(u32::MAX)),
            truncated: response.finish_reason() == Some(FinishReason::Length),
            ..ModelReply::default()
        };
        for content in response.choice {
            match content {
                AssistantContent::Text(said) => reply.text.push_str(&said.text),
                AssistantContent::ToolCall(call) => reply.tool_calls.push(ModelToolCall {
                    id: call.id.to_string(),
                    name: call.function.name.to_string(),
                    arguments: Value::Object(call.function.arguments),
                }),
                _ => {}
            }
        }
        Ok(reply)
    }
}

/// `messages` as Converse wants them: alternating, each side's run of ours one message.
fn conversation(messages: Vec<ModelMessage>) -> Result<Vec<Message>, ModelError> {
    let mut conversation: Vec<Message> = Vec::new();
    for message in messages {
        match message {
            ModelMessage::User(text) => user(&mut conversation, vec![UserContent::text(text)]),
            ModelMessage::ToolResults(results) => {
                let results = results
                    .into_iter()
                    .map(|result| {
                        Ok(UserContent::ToolResult(ToolResult {
                            call: CallId::from_wire(result.id),
                            name: tool_name(&result.name)?,
                            content: vec![ToolResultContent::text(result.content)],
                            is_error: result.is_error,
                        }))
                    })
                    .collect::<Result<Vec<_>, ModelError>>()?;
                user(&mut conversation, results);
            }
            ModelMessage::Assistant(text) => {
                assistant(&mut conversation, vec![AssistantContent::text(text)]);
            }
            ModelMessage::ToolCalls(calls) => {
                let calls = calls
                    .into_iter()
                    .map(|call| {
                        Ok(AssistantContent::tool_call(
                            call.id,
                            tool_name(&call.name)?,
                            call.arguments,
                        ))
                    })
                    .collect::<Result<Vec<_>, ModelError>>()?;
                assistant(&mut conversation, calls);
            }
        }
    }
    Ok(conversation)
}

fn user(conversation: &mut Vec<Message>, content: Vec<UserContent>) {
    if let Some(Message::User { content: last }) = conversation.last_mut() {
        last.extend(content);
    } else {
        conversation.push(Message::User { content });
    }
}

fn assistant(conversation: &mut Vec<Message>, content: Vec<AssistantContent>) {
    if let Some(Message::Assistant(last)) = conversation.last_mut() {
        last.content.extend(content);
    } else {
        conversation.push(Message::Assistant(AssistantMessage::new(content)));
    }
}
