//! The adapter of the assistant's tools port: the server's [`Toolbox`], presenting the token of
//! the request running the turn.

use crate::application::{Answered, AppTool, AppTools};
use app_web::mcp::{NotSent, Toolbox};
use app_web::{ErrorBody, ErrorCode};
use async_trait::async_trait;
use poem_openapi::types::ToJSON;
use serde_json::Value;

pub(super) struct CallerTools {
    toolbox: Toolbox,
    token: String,
}

impl CallerTools {
    pub(super) fn new(toolbox: Toolbox, token: &str) -> Self {
        Self {
            toolbox,
            token: token.to_owned(),
        }
    }
}

#[async_trait]
impl AppTools for CallerTools {
    fn app(&self) -> &str {
        self.toolbox.name()
    }

    fn instructions(&self) -> &str {
        self.toolbox.instructions()
    }

    fn tools(&self) -> Vec<AppTool> {
        self.toolbox
            .tools()
            .iter()
            .map(|tool| AppTool {
                name: tool.name.to_owned(),
                description: tool.description.to_owned(),
                input_schema: tool.input_schema.clone(),
                reads: tool.reads,
            })
            .collect()
    }

    async fn call(&self, name: &str, arguments: Value) -> Answered {
        match self.toolbox.call(name, arguments, &self.token).await {
            Ok(answer) => Answered {
                status: answer.status,
                body: answer.body,
            },
            Err(err @ NotSent::NoSuchTool(_)) => refused(404, ErrorCode::NotFound, &err),
            Err(err @ NotSent::Arguments(_)) => refused(400, ErrorCode::MalformedRequest, &err),
        }
    }
}

/// As the operation would have refused it, had there been one to send it to.
fn refused(status: u16, code: ErrorCode, err: &NotSent) -> Answered {
    Answered {
        status,
        body: ErrorBody::new(code, err.to_string())
            .to_json()
            .unwrap_or_default(),
    }
}
