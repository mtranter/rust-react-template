use super::PendingEvent;
use crate::domain::{Note, NoteEvent};
use app_events::EventPayload;
use notes_api::events::v1::{self, NoteArchived, NoteCreated, NoteEdited};

/// Where the domain's events meet the published contract.
pub(super) fn pending(note: &Note, event: NoteEvent) -> PendingEvent {
    let payload = EventPayload::new(state(note));
    match event {
        NoteEvent::Created => PendingEvent::of::<NoteCreated>(payload),
        NoteEvent::Edited => PendingEvent::of::<NoteEdited>(payload),
        NoteEvent::Archived => PendingEvent::of::<NoteArchived>(payload),
    }
}

fn state(note: &Note) -> v1::Note {
    v1::Note {
        id: note.id().as_uuid(),
        author_id: note.author_id().as_uuid(),
        title: note.title().to_string(),
        body: note.body().to_string(),
        archived_at: note.archived_at(),
    }
}
