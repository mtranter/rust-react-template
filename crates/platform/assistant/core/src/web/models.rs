use crate::application::Status;
use poem_openapi::Object;

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct AssistantStatusResponse {
    /// Whether this app has a model to chat with. Off, there is no assistant to talk to.
    pub on: bool,
}

impl From<Status> for AssistantStatusResponse {
    fn from(status: Status) -> Self {
        Self { on: status.on }
    }
}
