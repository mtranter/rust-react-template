use super::Theme;
use app_authz::{Resource, Value};

/// What can be done to the Theme, as its policies and grants name it (ADR 0040). Reading it is
/// public, so it is no action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum ThemeAction {
    Update,
}

impl Resource for Theme {
    const KIND: &'static str = "theme";
    const ATTRS: &'static [&'static str] = &[];
    type Action = ThemeAction;

    fn attr(&self, _name: &str) -> Option<Value> {
        None
    }
}
