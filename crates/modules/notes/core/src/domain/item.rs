use super::ItemId;

/// A line of a note's checklist: a child of its [`Note`](super::Note), loaded,
/// changed, saved and announced only through it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    id: ItemId,
    text: String,
}

impl Item {
    pub(super) fn new(id: ItemId, text: String) -> Self {
        Self { id, text }
    }

    /// Skips invariants: for repositories rebuilding persisted state only.
    pub fn restore(id: ItemId, text: String) -> Self {
        Self { id, text }
    }

    pub fn id(&self) -> ItemId {
        self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}
