use super::NoteSummary;
use app_authz::{Resource, Value};

/// What can be done with the copy of notes, as its policies and grants name it
/// (ADR 0040). A read module has no domain, so its resource is its read model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum NoteSummaryAction {
    Search,
}

impl Resource for NoteSummary {
    const KIND: &'static str = "noteSummary";
    const ATTRS: &'static [&'static str] = &["authorId"];
    type Action = NoteSummaryAction;

    fn attr(&self, name: &str) -> Option<Value> {
        match name {
            "authorId" => Some(self.author_id.to_string().into()),
            _ => None,
        }
    }
}
