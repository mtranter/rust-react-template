use crate::domain::Note;
use notes_api::events::v1;

pub(super) fn note(note: &Note) -> v1::Note {
    v1::Note {
        id: note.id().as_uuid(),
        author_id: note.author_id().as_uuid(),
        title: note.title().to_string(),
        body: note.body().to_string(),
        archived_at: note.archived_at(),
    }
}
