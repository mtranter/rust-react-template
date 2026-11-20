use super::ChatModel;
use crate::domain::{Assistant, AssistantAction};
use app_authz::{Authorizer, Principal};
use app_error::AppResult;
use std::sync::Arc;

#[derive(Clone)]
pub struct AssistantService {
    /// `None` while no model is configured: the assistant is off.
    model: Option<Arc<dyn ChatModel>>,
    authorizer: Arc<Authorizer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub on: bool,
}

impl AssistantService {
    pub fn new(model: Option<Arc<dyn ChatModel>>, authorizer: Arc<Authorizer>) -> Self {
        Self { model, authorizer }
    }

    pub fn status(&self, principal: &Principal) -> AppResult<Status> {
        self.authorizer
            .authorize::<Assistant>(principal, AssistantAction::Chat)?;
        Ok(Status {
            on: self.model.is_some(),
        })
    }
}
