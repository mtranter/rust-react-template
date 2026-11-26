use super::{NoteService, Tx};
use crate::domain::NoteId;
use app_authz::Principal;
use app_error::AppResult;
use app_persistence::{DeferredJob, Run};
use async_trait::async_trait;
use connections_api::{Auth, Declaration, HealthCheck};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The other system notes are shared to, which an admin connects (ADR 0050): a noticeboard
/// taking a bearer token, its pins at `PUT /pins/:id`.
pub const NOTICEBOARD: &str = "noticeboard";

pub fn noticeboard() -> Declaration {
    Declaration::new(NOTICEBOARD, "Noticeboard")
        .auth(Auth::Bearer)
        .health_check(HealthCheck::get("/health"))
}

/// Pin a note to the noticeboard as it is when the job runs: deferred by `share_note`, keyed
/// by the note, so sharing again before it ran shares once.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareNote {
    pub note_id: Uuid,
}

impl DeferredJob for ShareNote {
    const NAME: &'static str = "notes.share_note";
}

impl ShareNote {
    pub fn key(note: NoteId) -> String {
        note.to_string()
    }
}

/// As `System`, carrying out what the author asked for. Calls out here, in a job, never in the
/// request or an outbox handler: a failure is retried with the job's backoff.
#[async_trait]
impl Run<ShareNote> for NoteService {
    async fn run(&self, _tx: &mut Tx, job: ShareNote) -> AppResult<()> {
        self.pin(&Principal::System, NoteId::from_uuid(job.note_id))
            .await
    }
}
