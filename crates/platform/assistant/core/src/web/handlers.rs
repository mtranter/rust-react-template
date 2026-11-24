use super::models::{
    AssistantStatusResponse, ConversationResponse, ConversationSort, ConversationSummaryResponse,
    DecideToolCallRequest, SendMessageRequest, TurnEventResponse,
};
use super::tools::CallerTools;
use crate::application::{AssistantService, TurnEvent, Versioned, proposals, summary};
use crate::domain::{Conversation, ConversationId, ToolCallId};
use app_error::AppError;
use app_web::mcp::Toolbox;
use app_web::{ApiResult, Caller, Draining, ListResult, WithETag, if_match_version, paged, sorts};
use futures_util::stream::{self, BoxStream, StreamExt};
use poem::web::Data;
use poem_openapi::{
    ApiResponse, OpenApi, Tags,
    param::{Header, Path, Query},
    payload::{EventStream, Json},
};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;

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

fn call_id(raw: &str) -> ApiResult<ToolCallId> {
    raw.parse()
        .map_err(|_| AppError::validation("invalid tool call id").into())
}

/// A turn's events as a stream the server ends on shutdown; the turn runs on regardless.
fn followed(events: UnboundedReceiver<TurnEvent>, draining: &Draining) -> Turn {
    let events = stream::unfold(events, |mut events| async move {
        events.recv().await.map(|event| (event, events))
    })
    .map(TurnEventResponse::from);
    EventStream::new(draining.bounding(events).boxed())
}

fn tagged(conversation: Versioned<Conversation>) -> ConversationResult {
    let version = conversation.version;
    ConversationResult::new(conversation.into(), version)
}

/// With each tool call's summary, from the app's tools, and the proposal of each awaiting the
/// caller's approval, read through them as the caller.
async fn tagged_with_summaries(
    conversation: Versioned<Conversation>,
    toolbox: &Toolbox,
    caller: &Caller,
) -> ConversationResult {
    let version = conversation.version;
    let tools = CallerTools::new(toolbox.clone(), caller.token());
    let proposed = proposals(&conversation.entity, &tools).await;
    let response = ConversationResponse::from(conversation).summarised(|tool| {
        toolbox
            .tools()
            .iter()
            .find(|candidate| candidate.name == tool)
            .map(|found| summary(found.description).to_string())
    });
    ConversationResult::new(response.proposed(proposed), version)
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
        toolbox: Data<&Toolbox>,
        caller: Caller,
        id: Path<String>,
    ) -> ApiResult<ConversationResult> {
        let conversation = assistant
            .0
            .conversation(&caller, conversation_id(&id.0)?)
            .await?;
        Ok(tagged_with_summaries(conversation, toolbox.0, &caller).await)
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
    /// stored `awaiting_approval` and not run, and the turn ends there (`decide_tool_call` goes
    /// on). Any action still awaiting the caller's decision is rejected first. Requires
    /// `If-Match` with the version the client last read (`412` when stale). Refused before the
    /// stream with `503` while the assistant is off and `404` for another's conversation. A turn
    /// whose stream is dropped still runs to its end.
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
        Ok(followed(events, draining.0))
    }

    /// Approve or reject an action the assistant asked to take as the caller, stored
    /// `awaiting_approval`, and follow the turn it resumes as a stream of events, as
    /// `send_message` does, beginning with the decision. Approving runs the operation as the
    /// caller, once however often it is approved or retried; rejecting tells the assistant the
    /// caller declined it. The turn goes on once every action of its step is decided. Takes no
    /// `If-Match`: the call's status is its guard, so it is never `412` or `428`. Refused
    /// before the stream with `409` when the call isn't awaiting a decision, `503` while the
    /// assistant is off, and `404` for another's conversation or a call it doesn't hold.
    #[oai(
        path = "/conversations/:id/tool-calls/:tool_call_id/decision",
        method = "post",
        operation_id = "decide_tool_call"
    )]
    #[allow(clippy::too_many_arguments)]
    async fn decide(
        &self,
        assistant: Data<&AssistantService>,
        toolbox: Data<&Toolbox>,
        draining: Data<&Draining>,
        caller: Caller,
        id: Path<String>,
        tool_call_id: Path<String>,
        body: Json<DecideToolCallRequest>,
    ) -> ApiResult<Turn> {
        let tools = Arc::new(CallerTools::new(toolbox.0.clone(), caller.token()));
        let events = assistant
            .0
            .decide(
                &caller,
                conversation_id(&id.0)?,
                call_id(&tool_call_id.0)?,
                body.0.decision.into(),
                tools,
            )
            .await?;
        Ok(followed(events, draining.0))
    }
}
