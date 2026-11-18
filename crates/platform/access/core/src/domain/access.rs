use super::{Assignment, Grant, Role};
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

/// What can be done with roles: `admin`'s by `access`' own policies, as with grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum RoleAction {
    List,
    Create,
    Delete,
}

impl Resource for Role {
    const KIND: &'static str = "role";
    const ATTRS: &'static [&'static str] = &[];
    type Action = RoleAction;
    const VIEW: Option<RoleAction> = Some(RoleAction::List);

    fn attr(&self, _: &str) -> Option<Value> {
        None
    }
}

/// What can be done with assignments: `admin`'s by `access`' own policies, as with grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum AssignmentAction {
    Assign,
    Unassign,
}

impl Resource for Assignment {
    const KIND: &'static str = "assignment";
    const ATTRS: &'static [&'static str] = &[];
    type Action = AssignmentAction;
    const VIEW: Option<AssignmentAction> = None;

    fn attr(&self, _: &str) -> Option<Value> {
        None
    }
}
