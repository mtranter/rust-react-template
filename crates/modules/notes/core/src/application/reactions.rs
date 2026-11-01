use super::{NoteService, Tx};
use crate::domain::AuthorId;
use app_error::AppResult;
use app_persistence::Handle;
use async_trait::async_trait;
use users_api::events::v1::UserDeleted;

/// Notes outlive their author, read-only: deleting a user archives their notes.
#[async_trait]
impl Handle<UserDeleted> for NoteService {
    async fn handle(&self, tx: &mut Tx, event: UserDeleted) -> AppResult<()> {
        self.archive_notes_by(tx, AuthorId::from_uuid(event.user_id))
            .await
    }
}
