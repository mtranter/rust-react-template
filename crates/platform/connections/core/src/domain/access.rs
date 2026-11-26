use super::Connection;
use app_authz::{Resource, Value};

/// What can be done to a Connection, as its policies and grants name it (ADR 0040).
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum ConnectionAction {
    List,
    View,
    Edit,
    SetSecret,
    ClearSecret,
}

impl Resource for Connection {
    const KIND: &'static str = "connection";
    const ATTRS: &'static [&'static str] = &["key"];
    type Action = ConnectionAction;
    const VIEW: Option<ConnectionAction> = Some(ConnectionAction::View);

    fn attr(&self, name: &str) -> Option<Value> {
        match name {
            "key" => Some(self.key().as_str().to_owned().into()),
            _ => None,
        }
    }
}
