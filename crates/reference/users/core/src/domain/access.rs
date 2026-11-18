use super::User;
use app_authz::{Resource, Value};

/// What can be done to an account, as its policies and grants name it (ADR 0040).
/// Registering is public, so it is no action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum UserAction {
    List,
    View,
    Rename,
    Delete,
}

impl Resource for User {
    const KIND: &'static str = "user";
    const ATTRS: &'static [&'static str] = &["id"];
    type Action = UserAction;

    fn attr(&self, name: &str) -> Option<Value> {
        match name {
            "id" => Some(self.id().to_string().into()),
            _ => None,
        }
    }
}
