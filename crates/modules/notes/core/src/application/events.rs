use super::{PendingEvent, Version};
use crate::domain::{Note, NoteEvent};
use notes_api::events::v1::{self, NoteTopic};

/// Where the domain's events meet the published contract.
pub(super) fn pending(note: &Note, version: Version, event: NoteEvent) -> PendingEvent {
    let event = match event {
        NoteEvent::Created => v1::NoteEvent::Created,
        NoteEvent::Edited => v1::NoteEvent::Edited,
        NoteEvent::Archived => v1::NoteEvent::Archived,
        NoteEvent::Deleted => v1::NoteEvent::Deleted,
    };
    PendingEvent::on::<NoteTopic>(version, &state(note), event)
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
