use super::Grant;
use app_authz::{Resource, Value};

/// What can be done with grants. `admin` may do all of it by `access`' own policies, which no
/// grant or revocation changes (ADR 0040).
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum GrantAction {
    /// List the grants, and read the catalog they are made from.
    List,
    Create,
    Revoke,
}

impl Resource for Grant {
    const KIND: &'static str = "grant";
    const ATTRS: &'static [&'static str] = &[];
    type Action = GrantAction;
    const VIEW: Option<GrantAction> = Some(GrantAction::List);

    fn attr(&self, _: &str) -> Option<Value> {
        None
    }
}
