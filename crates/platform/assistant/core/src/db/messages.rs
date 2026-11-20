//! A conversation's messages and their tool calls, read and written beside its row: a child has
//! no repository of its own.

use crate::domain::{Content, Message, Outcome, ToolCall, ToolCallStatus};
use app_db::conn;
use app_error::{AppError, AppResult};
use app_persistence::Tx;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

const USER: &str = "user";
const ASSISTANT: &str = "assistant";
const TOOL_CALLS: &str = "tool_calls";
const OK: &str = "ok";
const ERROR: &str = "error";

/// Each of `conversations`' messages, oldest first.
pub(super) async fn of(
    pool: &PgPool,
    conversations: &[Uuid],
) -> AppResult<HashMap<Uuid, Vec<Message>>> {
    let mut calls = calls_of(pool, conversations).await?;
    let rows = sqlx::query!(
        r#"
        SELECT conversation_id, id, posted_at, kind, text FROM assistant.message
        WHERE conversation_id = ANY($1)
        ORDER BY conversation_id, position
        "#,
        conversations,
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::internal)?;

    let mut messages: HashMap<Uuid, Vec<Message>> = HashMap::new();
    for row in rows {
        let content = match (row.kind.as_str(), row.text) {
            (USER, Some(text)) => Content::User(text),
            (ASSISTANT, Some(text)) => Content::Assistant(text),
            (TOOL_CALLS, None) => Content::ToolCalls(calls.remove(&row.id).unwrap_or_default()),
            (kind, _) => {
                return Err(unknown(format!(
                    "message {} is of no known kind: {kind}",
                    row.id
                )));
            }
        };
        messages
            .entry(row.conversation_id)
            .or_default()
            .push(Message::restore(row.id.into(), row.posted_at, content));
    }
    Ok(messages)
}

/// Each tool-calls message's calls, by the message's id, in the order they were asked for.
async fn calls_of(
    pool: &PgPool,
    conversations: &[Uuid],
) -> AppResult<HashMap<Uuid, Vec<ToolCall>>> {
    let rows = sqlx::query!(
        r#"
        SELECT message_id, id, tool, arguments, status, outcome, result FROM assistant.tool_call
        WHERE conversation_id = ANY($1)
        ORDER BY conversation_id, position
        "#,
        conversations,
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::internal)?;

    let mut calls: HashMap<Uuid, Vec<ToolCall>> = HashMap::new();
    for row in rows {
        let status: ToolCallStatus = row
            .status
            .parse()
            .map_err(|_| unknown(format!("tool call {} has no known status", row.id)))?;
        let outcome = match (row.outcome.as_deref(), row.result) {
            (None, _) => None,
            (Some(OK), Some(result)) => Some(Outcome::Ok(result)),
            (Some(ERROR), Some(result)) => Some(Outcome::Error(result)),
            (Some(outcome), _) => {
                return Err(unknown(format!(
                    "tool call {} has no known outcome: {outcome}",
                    row.id
                )));
            }
        };
        calls.entry(row.message_id).or_default().push(ToolCall::new(
            row.id.into(),
            row.tool,
            row.arguments,
            status,
            outcome,
        ));
    }
    Ok(calls)
}

#[derive(Default)]
struct Messages {
    ids: Vec<Uuid>,
    posted_at: Vec<DateTime<Utc>>,
    kinds: Vec<&'static str>,
    texts: Vec<Option<String>>,
}

#[derive(Default)]
struct Calls {
    message_ids: Vec<Uuid>,
    ids: Vec<Uuid>,
    tools: Vec<String>,
    arguments: Vec<Value>,
    statuses: Vec<&'static str>,
    outcomes: Vec<Option<&'static str>>,
    results: Vec<Option<Value>>,
}

/// Replaces the stored messages with `messages`. Runs only once the conversation's own row
/// passed its version guard in the same `tx`, so no other writer races it.
pub(super) async fn replace(
    tx: &mut Tx,
    conversation: Uuid,
    messages: &[Message],
) -> AppResult<()> {
    sqlx::query!(
        "DELETE FROM assistant.message WHERE conversation_id = $1",
        conversation,
    )
    .execute(conn(tx))
    .await
    .map_err(AppError::internal)?;
    if messages.is_empty() {
        return Ok(());
    }

    let mut rows = Messages::default();
    let mut calls = Calls::default();
    for message in messages {
        rows.ids.push(message.id().as_uuid());
        rows.posted_at.push(message.posted_at());
        let (kind, text) = match message.content() {
            Content::User(text) => (USER, Some(text.clone())),
            Content::Assistant(text) => (ASSISTANT, Some(text.clone())),
            Content::ToolCalls(asked) => {
                for call in asked {
                    calls.message_ids.push(message.id().as_uuid());
                    calls.ids.push(call.id().as_uuid());
                    calls.tools.push(call.tool().to_string());
                    calls.arguments.push(call.arguments().clone());
                    calls.statuses.push(call.status().into());
                    let (outcome, result) = match call.outcome() {
                        None => (None, None),
                        Some(Outcome::Ok(body)) => (Some(OK), Some(body.clone())),
                        Some(Outcome::Error(body)) => (Some(ERROR), Some(body.clone())),
                    };
                    calls.outcomes.push(outcome);
                    calls.results.push(result);
                }
                (TOOL_CALLS, None)
            }
        };
        rows.kinds.push(kind);
        rows.texts.push(text);
    }

    sqlx::query!(
        r#"
        INSERT INTO assistant.message (conversation_id, id, position, posted_at, kind, text)
        SELECT $1, message.id, message.position::integer, message.posted_at, message.kind, message.text
        FROM unnest($2::uuid[], $3::timestamptz[], $4::text[], $5::text[])
            WITH ORDINALITY AS message (id, posted_at, kind, text, position)
        "#,
        conversation,
        &rows.ids,
        &rows.posted_at,
        &rows.kinds as &[&str],
        &rows.texts as &[Option<String>],
    )
    .execute(conn(tx))
    .await
    .map_err(AppError::internal)?;

    if calls.ids.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        r#"
        INSERT INTO assistant.tool_call
            (conversation_id, message_id, id, position, tool, arguments, status, outcome, result)
        SELECT $1, call.message_id, call.id, call.position::integer, call.tool, call.arguments,
            call.status, call.outcome, call.result
        FROM unnest($2::uuid[], $3::uuid[], $4::text[], $5::jsonb[], $6::text[], $7::text[], $8::jsonb[])
            WITH ORDINALITY AS call (message_id, id, tool, arguments, status, outcome, result, position)
        "#,
        conversation,
        &calls.message_ids,
        &calls.ids,
        &calls.tools,
        &calls.arguments,
        &calls.statuses as &[&str],
        &calls.outcomes as &[Option<&str>],
        &calls.results as &[Option<Value>],
    )
    .execute(conn(tx))
    .await
    .map_err(AppError::internal)?;
    Ok(())
}

/// A stored row this code can't read: a value the migrations' checks should have refused.
fn unknown(what: String) -> AppError {
    AppError::internal(std::io::Error::other(what))
}
