//! The engine's tests, over a test `Note` and ADR 0040's example files.

mod adr;
mod assigning;
mod combining;
mod grants;
mod refusals;
mod rows;
mod swapping;

use crate::{
    Filter, Grant, Policies, PolicyError, PolicyFile, Principal, Resource, RuleSet, Scope, Value,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum NoteAction {
    Create,
    List,
    View,
    Edit,
    Archive,
    #[strum(serialize = "items:add")]
    AddItem,
    #[strum(serialize = "items:remove")]
    RemoveItem,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub author_id: &'static str,
    pub archived: bool,
    pub status: &'static str,
    pub visibility: &'static str,
}

impl Resource for Note {
    const KIND: &'static str = "note";
    const ATTRS: &'static [&'static str] = &["authorId", "archived", "status", "visibility"];
    type Action = NoteAction;
    const VIEW: Option<NoteAction> = Some(NoteAction::View);

    fn attr(&self, name: &str) -> Option<Value> {
        match name {
            "authorId" => Some(self.author_id.into()),
            "archived" => Some(self.archived.into()),
            "status" => Some(self.status.into()),
            "visibility" => Some(self.visibility.into()),
            _ => None,
        }
    }
}

pub const ALICE: &str = "alice";
pub const BOB: &str = "bob";

pub fn note(author_id: &'static str, archived: bool) -> Note {
    Note {
        author_id,
        archived,
        status: "draft",
        visibility: "private",
    }
}

pub fn user(id: &str, roles: &[&str]) -> Principal {
    Principal::user(id, roles.iter().copied())
}

pub fn scopes() -> PolicyFile {
    crate::policy_file!("src/tests/notes_scopes.yaml")
}

pub fn note_policy() -> PolicyFile {
    crate::policy_file!("src/tests/note.yaml")
}

pub fn defaults() -> PolicyFile {
    crate::policy_file!("src/tests/default_grants.yaml")
}

/// ADR 0040's example files, as the `notebook` Module would load them.
pub fn adr_policies() -> Policies {
    Policies::builder("notebook")
        .resource::<Note>(note_policy())
        .derived_roles(scopes())
        .default_grants(defaults())
        .build()
        .expect("ADR 0040's examples load")
}

pub fn compile(policies: &Policies, grants: &[Grant]) -> RuleSet {
    let compiled = RuleSet::compile(std::slice::from_ref(policies), grants).expect("compiles");
    assert_eq!(compiled.stale, [], "no grant is stale");
    compiled.rule_set
}

/// A resource policy for `note` with the given rules, importing ADR 0040's scopes.
pub fn note_rules(rules: &str) -> PolicyFile {
    let rules = if rules.is_empty() { "    []\n" } else { rules };
    PolicyFile::new(
        "note.yaml",
        format!(
            "apiVersion: auth.flagrante.io/v1\nresourcePolicy:\n  resource: note\n  version: default\n  importDerivedRoles: [notes_scopes]\n  rules:\n{rules}"
        ),
    )
}

pub fn with_rules(rules: &str) -> Result<Policies, PolicyError> {
    Policies::builder("notebook")
        .resource::<Note>(note_rules(rules))
        .derived_roles(scopes())
        .build()
}

pub fn grant(role: &str, resource: &str, action: &str, scope: Scope) -> Grant {
    Grant {
        role: role.into(),
        resource: resource.into(),
        action: action.into(),
        scope,
    }
}

pub fn eq(attr: &str, value: impl Into<Value>) -> Filter {
    Filter::Eq(attr.into(), value.into())
}
