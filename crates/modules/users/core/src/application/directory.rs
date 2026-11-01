use super::UserService;
use crate::domain::UserId;
use app_error::AppResult;
use async_trait::async_trait;
use users_api::UserDirectory;
use uuid::Uuid;

#[async_trait]
impl UserDirectory for UserService {
    async fn exists(&self, id: Uuid) -> AppResult<bool> {
        UserService::exists(self, UserId::from_uuid(id)).await
    }
}
