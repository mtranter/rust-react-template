use app_authz::{Resource, Value};

/// The assistant itself, as its policies and grants name it (ADR 0040): what a caller may do
/// with it before any conversation exists.
pub struct Assistant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum AssistantAction {
    Chat,
}

impl Resource for Assistant {
    const KIND: &'static str = "assistant";
    const ATTRS: &'static [&'static str] = &[];
    type Action = AssistantAction;

    fn attr(&self, _name: &str) -> Option<Value> {
        None
    }
}
