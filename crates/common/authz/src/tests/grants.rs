use super::*;
use crate::{GrantRefusal, Plan, Scope};

fn editor_set() -> PolicyFile {
    PolicyFile::new(
        "editors.yaml",
        "apiVersion: auth.flagrante.io/v1\nderivedRoles:\n  name: editors\n  definitions:\n    - name: editor\n      parentRoles: [user]\n",
    )
}

fn policies() -> Policies {
    Policies::builder("notebook").capability(crate::tests::capability())
        .resource::<Note>(PolicyFile::new(
            "note.yaml",
            "apiVersion: auth.flagrante.io/v1\nresourcePolicy:\n  resource: note\n  version: default\n  importDerivedRoles: [notes_scopes, editors]\n  rules: []\n",
        ))
        .derived_roles(scopes())
        .derived_roles(editor_set())
        .build()
        .expect("loads")
}

#[test]
fn a_grant_checks_its_resource_action_and_scope() {
    let named = |s: &str| Scope::Named(s.into());
    let cases = [
        (grant("support", "note", "view", Scope::Any), Ok(())),
        (
            grant("support", "note", "items:add", named("author")),
            Ok(()),
        ),
        (
            grant("support", "module:notebook", "use", Scope::Any),
            Err(GrantRefusal::UnknownResource("module:notebook".into())),
        ),
        (
            grant("", "note", "view", Scope::Any),
            Err(GrantRefusal::NoRole),
        ),
        (
            grant("support", "task", "view", Scope::Any),
            Err(GrantRefusal::UnknownResource("task".into())),
        ),
        (
            grant("support", "module:tasks", "use", Scope::Any),
            Err(GrantRefusal::UnknownResource("module:tasks".into())),
        ),
        (
            grant("support", "note", "publish", Scope::Any),
            Err(GrantRefusal::UnknownAction {
                resource: "note".into(),
                action: "publish".into(),
            }),
        ),
        (
            grant("support", "note", "*", Scope::Any),
            Err(GrantRefusal::UnknownAction {
                resource: "note".into(),
                action: "*".into(),
            }),
        ),
        (
            grant("support", "note", "view", named("owner")),
            Err(GrantRefusal::UnknownScope {
                resource: "note".into(),
                scope: "owner".into(),
            }),
        ),
        (
            grant("support", "note", "view", named("editor")),
            Err(GrantRefusal::UnknownScope {
                resource: "note".into(),
                scope: "editor".into(),
            }),
        ),
    ];
    let policies = policies();
    for (grant, expected) in cases {
        let checked = policies.check(&grant).map_err(|refused| {
            assert_eq!(*refused.grant, grant);
            refused.reason
        });
        assert_eq!(checked, expected, "{grant}");
    }
}

#[test]
fn a_refused_grant_names_itself() {
    let refused = policies()
        .check(&grant("support", "note", "publish", Scope::Any))
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        "grant `support → note:publish, any` refused: `publish` is not an action of `note`"
    );
}

#[test]
fn a_stale_grant_is_skipped_and_returned() {
    let policies = adr_policies();
    let stale = vec![
        grant("support", "note", "publish", Scope::Any),
        grant("support", "note", "view", Scope::Named("team".into())),
        grant("support", "task", "view", Scope::Any),
    ];
    let mut grants = stale.clone();
    grants.push(grant("support", "note", "edit", Scope::Any));
    let compiled =
        crate::RuleSet::compile(std::slice::from_ref(&policies), &grants).expect("compiles");
    assert_eq!(compiled.stale, stale);
    let support = user(ALICE, &["support"]);
    assert_eq!(
        compiled.rule_set.plan::<Note>(&support, NoteAction::View),
        Plan::Denied
    );
    assert_eq!(
        compiled.rule_set.plan::<Note>(&support, NoteAction::Edit),
        Plan::Where(Filter::None(vec![eq("archived", true)]))
    );
}

#[test]
fn a_resource_in_two_modules_policies_fails_the_compile() {
    let policies = adr_policies();
    let twice = [policies.clone(), policies];
    assert_eq!(
        crate::RuleSet::compile(&twice, &[]).map(|_| ()),
        Err(crate::CompileError::DuplicateResource("note".into()))
    );
}
