use super::Caller;
use super::models::{
    AssistantStatusResponse, ConversationResponse, ConversationSort, ConversationSummaryResponse,
    SendMessageRequest, TurnEventResponse,
};
use super::tools::CallerTools;
use crate::application::{AssistantService, Versioned};
use crate::domain::{Conversation, ConversationId};
use app_error::AppError;
use app_web::mcp::Toolbox;
use app_web::{ApiResult, Draining, ListResult, WithETag, if_match_version, paged, sorts};
use futures_util::stream::{self, BoxStream, StreamExt};
use poem::web::Data;
use poem_openapi::{
    ApiResponse, OpenApi, Tags,
    param::{Header, Path, Query},
    payload::{EventStream, Json},
};
use std::sync::Arc;

type Turn = EventStream<BoxStream<'static, TurnEventResponse>>;

type ConversationResult = WithETag<ConversationResponse>;

#[derive(Tags)]
enum ApiTag {
    Assistant,
}

#[derive(ApiResponse)]
enum Deleted {
    #[oai(status = 204)]
    Deleted,
}

fn conversation_id(raw: &str) -> ApiResult<ConversationId> {
    raw.parse()
        .map_err(|_| AppError::validation("invalid conversation id").into())
}

fn tagged(conversation: Versioned<Conversation>) -> ConversationResult {
    let version = conversation.version;
    ConversationResult::new(conversation.into(), version)
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

    /// Start a conversation with the assistant, owned by the caller: empty and untitled until
    /// its first message. Refused with `403` to a caller who may not chat.
    #[oai(
        path = "/conversations",
        method = "post",
        operation_id = "start_conversation"
    )]
    async fn start(
        &self,
        assistant: Data<&AssistantService>,
        caller: Caller,
    ) -> ApiResult<ConversationResult> {
        Ok(tagged(assistant.0.start(&caller).await?))
    }

    /// List the caller's own conversations, without their messages, paginated. Newest first
    /// unless `sort` says otherwise (`-` prefix for descending). No one, an admin included,
    /// lists another's.
    #[oai(
        path = "/conversations",
        method = "get",
        operation_id = "list_conversations"
    )]
    async fn list(
        &self,
        assistant: Data<&AssistantService>,
        caller: Caller,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
        sort: Query<Option<Vec<ConversationSort>>>,
    ) -> ApiResult<Json<ListResult<ConversationSummaryResponse>>> {
        let query = paged(limit.0, offset.0).sorted(sorts(sort.0));
        let page = query.page;
        let listed = assistant.0.conversations(&caller, query).await?;
        Ok(Json(ListResult::new(listed, page)))
    }

    /// Fetch one of the caller's conversations with its messages, oldest first, and each tool
    /// call the assistant made in it. Its version is in the `ETag` header (and body). Another's
    /// conversation is refused with `404`.
    #[oai(
        path = "/conversations/:id",
        method = "get",
        operation_id = "get_conversation"
    )]
    async fn get(
        &self,
        assistant: Data<&AssistantService>,
        caller: Caller,
        id: Path<String>,
    ) -> ApiResult<ConversationResult> {
        let conversation = assistant
            .0
            .conversation(&caller, conversation_id(&id.0)?)
            .await?;
        Ok(tagged(conversation))
    }

    /// Delete one of the caller's conversations and its messages. Requires `If-Match` with the
    /// version the client last read; a stale value is refused with `412` and deletes nothing.
    /// Another's conversation is refused with `404`.
    #[oai(
        path = "/conversations/:id",
        method = "delete",
        operation_id = "delete_conversation"
    )]
    async fn delete(
        &self,
        assistant: Data<&AssistantService>,
        caller: Caller,
        id: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
    ) -> ApiResult<Deleted> {
        let expected = if_match_version(if_match.0)?;
        assistant
            .0
            .delete(&caller, conversation_id(&id.0)?, expected)
            .await?;
        Ok(Deleted::Deleted)
    }

    /// Say something to the assistant in one of the caller's conversations, and follow the turn
    /// it runs as a stream of events: its words as they are generated, each of the app's
    /// operations it calls as the caller and the answer, and the stored messages and new version
    /// when it ends. Operations that only read run at once; one that would change anything is
    /// stored `awaiting_approval` and not run, and the turn ends there. Requires `If-Match` with
    /// the version the client last read (`412` when stale). Refused before the stream with
    /// `503` while the assistant is off, `409` while an action awaits the caller's decision, and
    /// `404` for another's conversation. A turn whose stream is dropped still runs to its end.
    #[oai(
        path = "/conversations/:id/messages",
        method = "post",
        operation_id = "send_message"
    )]
    #[allow(clippy::too_many_arguments)]
    async fn send(
        &self,
        assistant: Data<&AssistantService>,
        toolbox: Data<&Toolbox>,
        draining: Data<&Draining>,
        caller: Caller,
        id: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
        body: Json<SendMessageRequest>,
    ) -> ApiResult<Turn> {
        let expected = if_match_version(if_match.0)?;
        let tools = Arc::new(CallerTools::new(toolbox.0.clone(), caller.token()));
        let events = assistant
            .0
            .send(
                &caller,
                conversation_id(&id.0)?,
                expected,
                &body.0.text,
                tools,
            )
            .await?;
        let events = stream::unfold(events, |mut events| async move {
            events.recv().await.map(|event| (event, events))
        })
        .map(TurnEventResponse::from);
        Ok(EventStream::new(draining.0.bounding(events).boxed()))
    }
}
