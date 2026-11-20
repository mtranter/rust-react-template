//! The platform's `assistant` module (Spec 0012): a chat in which a model acts in the app as the
//! signed-in user, on with a model configured (`APP_AGENT_MODEL`) and off without one. Platform,
//! not the app's: it stays registered after `make init`.

mod application;
mod bedrock;
mod di;
mod domain;
mod web;

pub use application::{ChatModel, ModelError, ModelMessage, ModelReply, ModelRequest, Settings};
pub use di::{AssistantProvider, module, policies, provider};
pub use domain::{Assistant, AssistantAction};
pub use web::AssistantEndpoints;
