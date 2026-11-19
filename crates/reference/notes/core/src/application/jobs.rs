use super::{NoteService, Tx};
use crate::domain::NoteId;
use app_authz::Principal;
use app_error::AppResult;
use app_persistence::{DeferredJob, Run};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Archive a note at the `archiveAt` its author chose: deferred by the use
/// case that sets it, keyed by the note, so a change reschedules it and
/// clearing it cancels it.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveNote {
    pub note_id: Uuid,
}

impl DeferredJob for ArchiveNote {
    const NAME: &'static str = "notes.archive_note";
}

impl ArchiveNote {
    pub fn key(note: NoteId) -> String {
        note.to_string()
    }
}

/// As `System`, carrying out what the author already decided.
#[async_trait]
impl Run<ArchiveNote> for NoteService {
    async fn run(&self, tx: &mut Tx, job: ArchiveNote) -> AppResult<()> {
        self.archive_when_due(&Principal::System, tx, NoteId::from_uuid(job.note_id))
            .await
    }
}
