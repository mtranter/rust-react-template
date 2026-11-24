//! A module of the tests' own, in two releases, for `access` to grant from: the second drops
//! the `polish` action and adds a default grant of `inspect`.

use access::{Modules, Operation};
use app_authz::{Policies, PolicyFile, Resource, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Release {
    First,
    Second,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum GadgetAction {
    Make,
    Polish,
    Scrap,
}

pub struct Gadget;

impl Resource for Gadget {
    const KIND: &'static str = "gadget";
    const ATTRS: &'static [&'static str] = &["makerId"];
    type Action = GadgetAction;

    fn attr(&self, _: &str) -> Option<Value> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum GadgetActionLater {
    Make,
    Inspect,
    Scrap,
}

pub struct GadgetLater;

impl Resource for GadgetLater {
    const KIND: &'static str = "gadget";
    const ATTRS: &'static [&'static str] = &["makerId"];
    type Action = GadgetActionLater;

    fn attr(&self, _: &str) -> Option<Value> {
        None
    }
}

const SCOPES: &str = "apiVersion: auth.flagrante.io/v1
derivedRoles:
  name: workshop_scopes
  definitions:
    - name: maker
      parentRoles: [\"*\"]
      condition:
        match:
          expr: R.attr.makerId == P.id
";

const GADGET: &str = "apiVersion: auth.flagrante.io/v1
resourcePolicy:
  resource: gadget
  version: default
  importDerivedRoles: [workshop_scopes]
  rules: []
";

const FIRST_DEFAULTS: &str = "apiVersion: auth.flagrante.io/v1
defaultGrants:
  - { role: user, resource: gadget, action: make }
  - { role: user, resource: gadget, action: polish, scope: maker }
  - { role: admin, resource: gadget, actions: [make, polish] }
  - { role: maintainer, resource: gadget, action: polish }
";

const SECOND_DEFAULTS: &str = "apiVersion: auth.flagrante.io/v1
defaultGrants:
  - { role: user, resource: gadget, action: make }
  - { role: user, resource: gadget, action: inspect }
  - { role: admin, resource: gadget, actions: [make] }
";

/// What `access` is built from in `release`: its own policies and the workshop's.
pub fn modules(release: Release) -> Modules {
    let builder = Policies::builder("workshop")
        .derived_roles(PolicyFile::new("policies/derived_roles.yaml", SCOPES));
    let workshop = match release {
        Release::First => builder
            .resource::<Gadget>(PolicyFile::new("policies/gadget.yaml", GADGET))
            .default_grants(PolicyFile::new(
                "policies/default_grants.yaml",
                FIRST_DEFAULTS,
            )),
        Release::Second => builder
            .resource::<GadgetLater>(PolicyFile::new("policies/gadget.yaml", GADGET))
            .default_grants(PolicyFile::new(
                "policies/default_grants.yaml",
                SECOND_DEFAULTS,
            )),
    };
    Modules {
        policies: vec![
            access::policies().build().expect("access' policies load"),
            workshop.build().expect("the workshop's policies load"),
        ],
        operations: vec![
            Operation {
                id: "make_gadget",
                resource: "gadget",
                action: "make",
            },
            Operation {
                id: "list_grants",
                resource: "grant",
                action: "list",
            },
        ],
    }
}

/// The default grants of the first release, as `list_grants` answers them: `(role, resource,
/// action, scope)`.
pub const FIRST_RELEASE_DEFAULTS: &[(&str, &str, &str, &str)] = &[
    ("user", "gadget", "make", "any"),
    ("user", "gadget", "polish", "maker"),
    ("admin", "gadget", "make", "any"),
    ("admin", "gadget", "polish", "any"),
    ("maintainer", "gadget", "polish", "any"),
];

/// The role the first release's default grants name that isn't built in, which `migrate`
/// creates.
pub const MAINTAINER: &str = "maintainer";
