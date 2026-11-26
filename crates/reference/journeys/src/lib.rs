//! The reference modules served as one app, for their journeys: the template's
//! server registers the same, and an app's server none of them. The platform's
//! `access`, `assistant` and `connections` are served beside them, as every app's server
//! registers them, the assistant with [`Directed`] in Bedrock's place.

use app_module::Module;
use assistant::{
    ChatModel, ModelError, ModelMessage, ModelReply, ModelRequest, ModelToolCall, Settings,
};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use server::Composition;
use std::sync::Arc;

pub struct Reference;

impl Composition for Reference {
    type Endpoints = (
        access::AccessEndpoints,
        assistant::AssistantEndpoints,
        connections::ConnectionsEndpoints,
        users::UsersEndpoints,
        notes::NotesEndpoints,
        note_search::NoteSearchEndpoints,
    );

    fn modules() -> Vec<Module> {
        vec![
            access::module(Self::modules),
            assistant::module_with(Settings::default, |_| Some(Arc::new(Directed))),
            connections::module(),
            users::module(access::assignments),
            notes::module(users::directory, connections::clients),
            note_search::module(),
        ]
    }
}

/// Stands in for Bedrock, which no journey calls, doing as the user's words direct: words that
/// are `{"tool": .., "arguments": ..}` ask for that tool, and anything else is answered as said.
/// Given tool results, it answers with them.
pub struct Directed;

#[derive(Deserialize)]
struct Direction {
    tool: String,
    arguments: Value,
}

#[async_trait]
impl ChatModel for Directed {
    async fn reply(
        &self,
        request: ModelRequest,
        text: &(dyn for<'a> Fn(&'a str) + Send + Sync),
    ) -> Result<ModelReply, ModelError> {
        let reply = match request.messages.last() {
            Some(ModelMessage::User(said)) => match serde_json::from_str::<Direction>(said) {
                Ok(direction) => ModelReply {
                    tool_calls: vec![ModelToolCall {
                        id: "directed".to_string(),
                        name: direction.tool,
                        arguments: direction.arguments,
                    }],
                    ..ModelReply::default()
                },
                Err(_) => said_back(said.clone()),
            },
            Some(ModelMessage::ToolResults(results)) => said_back(
                results
                    .iter()
                    .map(|result| result.content.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => return Err(ModelError("nothing directs a reply".to_string())),
        };
        text(&reply.text);
        Ok(reply)
    }
}

fn said_back(text: String) -> ModelReply {
    ModelReply {
        text,
        ..ModelReply::default()
    }
}
