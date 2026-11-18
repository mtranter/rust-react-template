use super::Note;
use app_authz::{Resource, Value};

/// What can be done to a note, as its policies and grants name it (ADR 0040).
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum NoteAction {
    Create,
    List,
    View,
    Edit,
    Delete,
    /// Only the `UserDeleted` reaction archives, as `System`.
    Archive,
    #[strum(serialize = "items:add")]
    AddItem,
    #[strum(serialize = "items:remove")]
    RemoveItem,
}

impl Resource for Note {
    const KIND: &'static str = "note";
    const ATTRS: &'static [&'static str] = &["authorId", "archived"];
    type Action = NoteAction;

    fn attr(&self, name: &str) -> Option<Value> {
        match name {
            "authorId" => Some(self.author_id().to_string().into()),
            "archived" => Some(self.archived_at().is_some().into()),
            _ => None,
        }
    }
}
