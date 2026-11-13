use super::{PendingEvent, Version};
use crate::domain::{ItemId, Note, NoteEvent};
use notes_api::events::v1::{self, NoteTopic};

/// Where the domain's events meet the published contract.
pub(super) fn pending(note: &Note, version: Version, event: NoteEvent) -> PendingEvent {
    let event = match event {
        NoteEvent::Created => v1::NoteEvent::Created,
        NoteEvent::Edited => v1::NoteEvent::Edited,
        NoteEvent::Archived => v1::NoteEvent::Archived,
        NoteEvent::Deleted => v1::NoteEvent::Deleted,
        NoteEvent::ItemAdded(item) => v1::NoteEvent::ItemAdded(of(item)),
        NoteEvent::ItemRemoved(item) => v1::NoteEvent::ItemRemoved(of(item)),
    };
    PendingEvent::on::<NoteTopic>(version, &state(note), event)
}

fn of(item: ItemId) -> v1::OfItem {
    v1::OfItem {
        item_id: item.as_uuid(),
    }
}

fn state(note: &Note) -> v1::Note {
    v1::Note {
        id: note.id().as_uuid(),
        author_id: note.author_id().as_uuid(),
        title: note.title().to_string(),
        body: note.body().to_string(),
        archived_at: note.archived_at(),
        items: note
            .items()
            .iter()
            .map(|item| v1::Item {
                id: item.id().as_uuid(),
                text: item.text().to_string(),
            })
            .collect(),
    }
}
