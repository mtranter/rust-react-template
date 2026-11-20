//! Ids are `well_known_id(MODULE, kind, sequence)` (ADR 0028); kind `1` is a conversation, `2`
//! a message, `3` a tool call. Conversations are owned by subjects, which no module's table holds all of,
//! so the seeded owners are plain subjects.

// Seeding wires a pool to a repository, like `di.rs`.
#![allow(clippy::disallowed_types)]

use crate::application::{ConversationRepository, EventPublisher, Expected, Transactions, pending};
use crate::db::PgConversationRepository;
use crate::domain::{
    Conversation, ConversationEvent, ConversationId, Message, MessageId, Outcome, ToolCall,
    ToolCallId, ToolCallStatus,
};
use app_db::{PgEventPublisher, PgTransactions};
use app_domain::{FixedClock, seed_time, well_known_id};
use app_error::AppResult;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

/// Unique across modules; `make check-modules` enforces it. The platform's modules number from
/// 1000, clear of the app's, which `make new-module` numbers from 1.
pub const MODULE: u32 = 1000;

/// The subject `make dev` signs every request in as (`frontend/vite.config.ts`).
pub const SUBJECT_ALICE: &str = "00000001-0001-0000-0000-000000000001";
pub const SUBJECT_BOB: &str = "00000001-0001-0000-0000-000000000002";

pub const ALICE_CONVERSATION_ID_ROLES: ConversationId =
    ConversationId::from_uuid(well_known_id(MODULE, 1, 1));
pub const ALICE_CONVERSATION_ID_UNTITLED: ConversationId =
    ConversationId::from_uuid(well_known_id(MODULE, 1, 2));
pub const BOB_CONVERSATION_ID_GREETING: ConversationId =
    ConversationId::from_uuid(well_known_id(MODULE, 1, 3));

pub const ROLES_MESSAGE_ID_QUESTION: MessageId = MessageId::from_uuid(well_known_id(MODULE, 2, 1));
pub const ROLES_MESSAGE_ID_LOOKUP: MessageId = MessageId::from_uuid(well_known_id(MODULE, 2, 2));
pub const ROLES_MESSAGE_ID_ANSWER: MessageId = MessageId::from_uuid(well_known_id(MODULE, 2, 3));
pub const GREETING_MESSAGE_ID_HELLO: MessageId = MessageId::from_uuid(well_known_id(MODULE, 2, 4));
pub const LOOKUP_TOOL_CALL_ID_LIST_ROLES: ToolCallId =
    ToolCallId::from_uuid(well_known_id(MODULE, 3, 1));

fn started(id: ConversationId, owner: &str, minutes: i64) -> Conversation {
    Conversation::start(id, owner.into(), seed_time(minutes)).0
}

fn posted(conversation: &mut Conversation, message: Message) {
    let _ = conversation.post(message);
}

/// Alice asked which roles there are, and the assistant looked them up and answered.
pub fn alices_roles_conversation() -> Conversation {
    let mut conversation = started(ALICE_CONVERSATION_ID_ROLES, SUBJECT_ALICE, 0);
    let valid = "seed data satisfies the domain's invariants";
    posted(
        &mut conversation,
        Message::user(
            ROLES_MESSAGE_ID_QUESTION,
            "Which roles are there?",
            seed_time(1),
        )
        .expect(valid),
    );
    let lookup = ToolCall::new(
        LOOKUP_TOOL_CALL_ID_LIST_ROLES,
        "list_roles".into(),
        json!({}),
        ToolCallStatus::Ran,
        Some(Outcome::Ok(json!({
            "items": [{ "name": "admin" }, { "name": "user" }],
            "total": 2,
            "limit": 20,
            "offset": 0,
        }))),
    );
    posted(
        &mut conversation,
        Message::tool_calls(ROLES_MESSAGE_ID_LOOKUP, vec![lookup], seed_time(1)).expect(valid),
    );
    posted(
        &mut conversation,
        Message::assistant(
            ROLES_MESSAGE_ID_ANSWER,
            "There are two roles: **admin** and **user**.",
            seed_time(1),
        )
        .expect(valid),
    );
    conversation
}

/// Started after the other, and never written in.
pub fn alices_untitled_conversation() -> Conversation {
    started(ALICE_CONVERSATION_ID_UNTITLED, SUBJECT_ALICE, 2)
}

pub fn bobs_conversation() -> Conversation {
    let mut conversation = started(BOB_CONVERSATION_ID_GREETING, SUBJECT_BOB, 3);
    posted(
        &mut conversation,
        Message::user(GREETING_MESSAGE_ID_HELLO, "Hello", seed_time(3))
            .expect("seed data satisfies the domain's invariants"),
    );
    conversation
}

pub fn conversations() -> Vec<Conversation> {
    vec![
        alices_roles_conversation(),
        alices_untitled_conversation(),
        bobs_conversation(),
    ]
}

/// Saves only what's missing, so a deployed environment can re-run it on every release without
/// failing or undoing changes to seeded conversations. Announces each one it saves, as of when it
/// was started.
pub async fn standard(pool: &PgPool) -> AppResult<()> {
    save_missing(pool, conversations()).await
}

/// Saves each of `conversations` that isn't already, announcing it as [`standard`] does.
pub async fn save_missing(pool: &PgPool, conversations: Vec<Conversation>) -> AppResult<()> {
    let repo = PgConversationRepository::new(pool.clone());
    let mut tx = PgTransactions::new(pool.clone()).begin().await?;
    for conversation in conversations {
        if repo.get(conversation.id()).await?.is_none() {
            let version = repo.save(&mut tx, &conversation, Expected::New).await?;
            PgEventPublisher::new(Arc::new(FixedClock::new(conversation.created_at())))
                .publish(
                    &mut tx,
                    pending(&conversation, version, ConversationEvent::Started),
                )
                .await?;
        }
    }
    tx.commit().await
}
