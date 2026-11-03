/// What happened to a [`Note`](super::Note), as its transitions decide; the
/// service announces each one as the matching `notes_api` event.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteEvent {
    Created,
    Edited,
    Archived,
    Deleted,
}
