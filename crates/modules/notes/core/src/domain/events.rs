use super::ItemId;

/// What happened to a [`Note`](super::Note), as its transitions decide; the
/// service announces each one as the matching `notes_api` event. A change to
/// an item is the note's: it names the item, which the note's state can't.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteEvent {
    Created,
    Edited,
    Archived,
    Deleted,
    ItemAdded(ItemId),
    ItemRemoved(ItemId),
}
