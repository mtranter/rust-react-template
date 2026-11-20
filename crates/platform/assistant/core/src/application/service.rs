use super::{ChatModel, ConversationRepository, EventPublisher, Transactions};
use crate::domain::{Assistant, AssistantAction};
use app_authz::{Authorizer, Principal};
use app_domain::Clock;
use app_error::AppResult;
use std::sync::Arc;

/// Holds trait objects rather than being generic, so the service is one concrete type that can
/// be named and shared. Its conversations' use cases are in `conversations.rs`.
#[derive(Clone)]
pub struct AssistantService {
    /// `None` while no model is configured: the assistant is off.
    model: Option<Arc<dyn ChatModel>>,
    pub(super) conversations: Arc<dyn ConversationRepository>,
    pub(super) publisher: Arc<dyn EventPublisher>,
    pub(super) transactions: Arc<dyn Transactions>,
    pub(super) clock: Arc<dyn Clock>,
    pub(super) authorizer: Arc<Authorizer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub on: bool,
}

impl AssistantService {
    pub fn new(
        model: Option<Arc<dyn ChatModel>>,
        conversations: Arc<dyn ConversationRepository>,
        publisher: Arc<dyn EventPublisher>,
        transactions: Arc<dyn Transactions>,
        clock: Arc<dyn Clock>,
        authorizer: Arc<Authorizer>,
    ) -> Self {
        Self {
            model,
            conversations,
            publisher,
            transactions,
            clock,
            authorizer,
        }
    }

    pub fn status(&self, principal: &Principal) -> AppResult<Status> {
        self.authorizer
            .authorize::<Assistant>(principal, AssistantAction::Chat)?;
        Ok(Status {
            on: self.model.is_some(),
        })
    }
}
