use app_error::AppResult;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait UserDirectory: Send + Sync {
    async fn exists(&self, id: Uuid) -> AppResult<bool>;
}
