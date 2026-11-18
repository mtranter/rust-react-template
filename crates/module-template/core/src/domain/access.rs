use super::Widget;
use app_authz::{Resource, Value};

/// What can be done to a widget, as its policies and grants name it (ADR 0040).
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum WidgetAction {
    Create,
    View,
    Rename,
    List,
    Search,
}

impl Resource for Widget {
    const KIND: &'static str = "widget";
    /// What a condition in its policy may name: add an attribute here and in
    /// `attr` before a scope or rule uses it.
    const ATTRS: &'static [&'static str] = &["id"];
    type Action = WidgetAction;
    const VIEW: Option<WidgetAction> = Some(WidgetAction::View);

    fn attr(&self, name: &str) -> Option<Value> {
        match name {
            "id" => Some(self.id().to_string().into()),
            _ => None,
        }
    }
}
