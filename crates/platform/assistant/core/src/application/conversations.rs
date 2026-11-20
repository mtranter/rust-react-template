use super::{AssistantService, ConversationSortKey, ConversationSummary, events};
use super::{Expected, ListQuery, Listed, Version, Versioned};
use crate::domain::{Assistant, AssistantAction, Conversation, ConversationAction, ConversationId};
use app_authz::{Plan, Principal};
use app_error::{AppError, AppResult};

/// A conversation is its owner's alone: every use case asks for `chat` on the assistant, then
/// for its action on the conversation, scoped to its owner (`policies/`).
impl AssistantService {
    fn authorize_conversation(
        &self,
        principal: &Principal,
        action: ConversationAction,
    ) -> AppResult<Plan> {
        self.authorizer
            .authorize::<Assistant>(principal, AssistantAction::Chat)?;
        self.authorizer.authorize::<Conversation>(principal, action)
    }

    /// Owned by the caller, empty and untitled.
    pub async fn start(&self, principal: &Principal) -> AppResult<Versioned<Conversation>> {
        self.authorize_conversation(principal, ConversationAction::Start)?;
        let owner = principal.id().ok_or_else(|| {
            AppError::validation("only a signed-in user can start a conversation")
        })?;
        let (conversation, event) =
            Conversation::start(ConversationId::new(), owner.to_string(), self.clock.now());
        let mut tx = self.transactions.begin().await?;
        let version = self
            .conversations
            .save(&mut tx, &conversation, Expected::New)
            .await?;
        self.publisher
            .publish(&mut tx, events::pending(&conversation, version, event))
            .await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: conversation,
            version,
        })
    }

    pub async fn conversation(
        &self,
        principal: &Principal,
        id: ConversationId,
    ) -> AppResult<Versioned<Conversation>> {
        let plan = self.authorize_conversation(principal, ConversationAction::View)?;
        self.load_for(principal, &plan, id).await
    }

    pub async fn conversations(
        &self,
        principal: &Principal,
        query: ListQuery<ConversationSortKey>,
    ) -> AppResult<Listed<Versioned<ConversationSummary>>> {
        self.authorizer
            .authorize::<Assistant>(principal, AssistantAction::Chat)?;
        let rows = self
            .authorizer
            .rows::<Conversation>(principal, ConversationAction::List)?;
        self.conversations.list(&query, &rows).await
    }

    /// Removes it, its messages with it, and announces it as it was last.
    pub async fn delete(
        &self,
        principal: &Principal,
        id: ConversationId,
        expected: Version,
    ) -> AppResult<()> {
        let plan = self.authorize_conversation(principal, ConversationAction::Delete)?;
        let current = self.load_for(principal, &plan, id).await?;
        // Checked here as well as by the delete, so the state announced is the version deleted.
        if current.version != expected {
            return Err(AppError::precondition_failed(
                "the conversation was changed meanwhile",
            ));
        }
        let event = current.entity.delete();
        let mut tx = self.transactions.begin().await?;
        self.conversations.delete(&mut tx, id, expected).await?;
        self.publisher
            .publish(
                &mut tx,
                events::pending(&current.entity, expected.next(), event),
            )
            .await?;
        tx.commit().await
    }

    /// The conversation, if `plan` allows it: another's is not found.
    async fn load_for(
        &self,
        principal: &Principal,
        plan: &Plan,
        id: ConversationId,
    ) -> AppResult<Versioned<Conversation>> {
        let conversation = self
            .conversations
            .get(id)
            .await?
            .ok_or_else(|| AppError::not_found("conversation"))?;
        self.authorizer
            .check(principal, plan, &conversation.entity)?;
        Ok(conversation)
    }
}
