use super::{NoteService, Tx};
use app_authz::Principal;
use app_error::AppResult;
use app_persistence::{RunScheduled, Schedule};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Every night at 03:00 UTC, delete the notes archived more than
/// [`PURGE_AFTER`](super::service::PURGE_AFTER) before.
pub struct PurgeArchivedNotes;

impl Schedule for PurgeArchivedNotes {
    const NAME: &'static str = "notes.purge_archived_notes";
    const CRON: &'static str = "0 3 * * *";
}

/// As `System`: housekeeping no one asked for at the time.
#[async_trait]
impl RunScheduled<PurgeArchivedNotes> for NoteService {
    async fn run(&self, tx: &mut Tx, slot: DateTime<Utc>) -> AppResult<()> {
        self.purge_archived(&Principal::System, tx, slot).await
    }
}
