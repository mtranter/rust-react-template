use super::Caller;
use super::models::AssistantStatusResponse;
use crate::application::AssistantService;
use app_web::ApiResult;
use poem::web::Data;
use poem_openapi::{OpenApi, Tags, payload::Json};

#[derive(Tags)]
enum ApiTag {
    Assistant,
}

/// Unit struct so `dump-spec` can build the spec without a database.
#[derive(Default)]
pub struct AssistantEndpoints;

#[OpenApi(prefix_path = "/assistant", tag = "ApiTag::Assistant")]
impl AssistantEndpoints {
    /// Whether the assistant is on: whether this app has a model configured for the caller to
    /// chat with. Refused with `403` to a caller who may not chat.
    #[oai(
        path = "/status",
        method = "get",
        operation_id = "get_assistant_status"
    )]
    async fn status(
        &self,
        assistant: Data<&AssistantService>,
        caller: Caller,
    ) -> ApiResult<Json<AssistantStatusResponse>> {
        Ok(Json(assistant.0.status(&caller)?.into()))
    }
}
