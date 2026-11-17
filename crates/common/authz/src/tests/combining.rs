use super::*;
use crate::policies::action_matches;
use crate::{Plan, Scope};

fn rule(effect: &str, actions: &str, roles: &str, condition: Option<&str>) -> String {
    let mut r =
        format!("    - actions: {actions}\n      effect: EFFECT_{effect}\n      roles: {roles}\n");
    if let Some(expr) = condition {
        r.push_str(&format!(
            "      condition:\n        match:\n          expr: {expr}\n"
        ));
    }
    r
}

fn plan(rules: &[String], grants: &[Grant], principal: &Principal, action: NoteAction) -> Plan {
    let policies = with_rules(&rules.concat()).expect("loads");
    compile(&policies, grants).plan::<Note>(principal, action)
}

#[test]
fn deny_beats_allow_within_a_role_and_allow_wins_across_roles() {
    let archived = Some("R.attr.archived == true");
    let user_edits = grant("user", "note", "edit", Scope::Any);
    type Case<'a> = (&'a str, Vec<String>, Vec<Grant>, &'a [&'a str], Plan);
    let cases: Vec<Case> = vec![
        ("nothing allows", vec![], vec![], &["user"], Plan::Denied),
        (
            "an allow",
            vec![rule("ALLOW", "[edit]", "[user]", None)],
            vec![],
            &["user"],
            Plan::Allowed,
        ),
        (
            "a deny in the same role",
            vec![
                rule("ALLOW", "[edit]", "[user]", None),
                rule("DENY", "[edit]", "[user]", None),
            ],
            vec![],
            &["user"],
            Plan::Denied,
        ),
        (
            "a deny over a grant in the same role",
            vec![rule("DENY", "[edit]", "[user]", None)],
            vec![user_edits.clone()],
            &["user"],
            Plan::Denied,
        ),
        (
            "a deny through `*` over a grant",
            vec![rule("DENY", "[edit]", "[\"*\"]", None)],
            vec![user_edits.clone()],
            &["user"],
            Plan::Denied,
        ),
        (
            "a conditional deny over a grant",
            vec![rule("DENY", "[edit]", "[user]", archived)],
            vec![user_edits.clone()],
            &["user"],
            Plan::Where(Filter::None(vec![eq("archived", true)])),
        ),
        (
            "a deny in another role",
            vec![rule("DENY", "[edit]", "[user]", None)],
            vec![grant("admin", "note", "edit", Scope::Any)],
            &["user", "admin"],
            Plan::Allowed,
        ),
        (
            "a deny with no allow in its own role",
            vec![
                rule("DENY", "[edit]", "[auditor]", None),
                rule("ALLOW", "[edit]", "[user]", None),
            ],
            vec![],
            &["auditor", "user"],
            Plan::Allowed,
        ),
        (
            "a deny for another action",
            vec![rule("DENY", "[archive]", "[user]", None)],
            vec![user_edits.clone()],
            &["user"],
            Plan::Allowed,
        ),
        (
            "a grant to another role",
            vec![],
            vec![grant("admin", "note", "edit", Scope::Any)],
            &["user"],
            Plan::Denied,
        ),
    ];
    for (case, rules, grants, roles, expected) in cases {
        assert_eq!(
            plan(&rules, &grants, &user(ALICE, roles), NoteAction::Edit),
            expected,
            "{case}"
        );
    }
}

#[test]
fn wildcards_match_actions_and_roles() {
    let cases = [
        ("*", "edit", true),
        ("*", "items:add", true),
        ("items:*", "items:add", true),
        ("items:*", "items", false),
        ("items:*", "edit", false),
        ("*:add", "items:add", true),
        ("*:add", "items:remove", false),
        ("items:*", "items:add:all", false),
        ("edit", "edit", true),
        ("edit", "edits", false),
    ];
    for (pattern, action, expected) in cases {
        assert_eq!(
            action_matches(pattern, action),
            expected,
            "{pattern} ~ {action}"
        );
    }

    let rules = [rule("ALLOW", "[\"items:*\"]", "[\"*\"]", None)];
    let anyone = user(ALICE, &["whoever"]);
    assert_eq!(
        plan(&rules, &[], &anyone, NoteAction::AddItem),
        Plan::Allowed
    );
    assert_eq!(
        plan(&rules, &[], &anyone, NoteAction::RemoveItem),
        Plan::Allowed
    );
    assert_eq!(plan(&rules, &[], &anyone, NoteAction::Edit), Plan::Denied);
    assert_eq!(
        plan(&rules, &[], &user(ALICE, &[]), NoteAction::AddItem),
        Plan::Denied
    );
}

#[test]
fn scopes_apply_as_derived_roles_and_as_grants() {
    let author = || Plan::Where(eq("authorId", ALICE));
    let derived =
        "    - actions: [view]\n      effect: EFFECT_ALLOW\n      derivedRoles: [author]\n";
    assert_eq!(
        plan(
            &[derived.into()],
            &[],
            &user(ALICE, &["user"]),
            NoteAction::View
        ),
        author()
    );
    assert_eq!(
        plan(&[derived.into()], &[], &user(ALICE, &[]), NoteAction::View),
        Plan::Denied
    );

    let granted = [grant("user", "note", "view", Scope::Named("author".into()))];
    assert_eq!(
        plan(&[], &granted, &user(ALICE, &["user"]), NoteAction::View),
        author()
    );
    assert_eq!(
        plan(&[], &granted, &user(ALICE, &["admin"]), NoteAction::View),
        Plan::Denied
    );

    let both = [
        granted[0].clone(),
        grant("user", "note", "view", Scope::Any),
    ];
    assert_eq!(
        plan(&[], &both, &user(ALICE, &["user"]), NoteAction::View),
        Plan::Allowed
    );
}

#[test]
fn a_derived_role_applies_only_to_its_parent_roles() {
    let policies = Policies::builder("notes")
        .resource::<Note>(note_rules(
            "    - actions: [edit]\n      effect: EFFECT_ALLOW\n      derivedRoles: [own_draft]\n      condition:\n        match:\n          expr: R.attr.archived == false\n",
        ))
        .derived_roles(scopes())
        .derived_roles(PolicyFile::new(
            "drafts.yaml",
            "apiVersion: auth.flagrante.io/v1\nderivedRoles:\n  name: drafts\n  definitions:\n    - name: own_draft\n      parentRoles: [editor]\n      condition:\n        match:\n          all:\n            of:\n              - expr: R.attr.authorId == P.id\n              - expr: R.attr.status in [\"draft\", \"review\"]\n",
        ))
        .build();
    // `drafts` isn't imported by note.yaml.
    assert!(
        policies
            .unwrap_err()
            .reason
            .contains("not a derived role imported")
    );

    let policies = Policies::builder("notes")
        .resource::<Note>(PolicyFile::new(
            "note.yaml",
            "apiVersion: auth.flagrante.io/v1\nresourcePolicy:\n  resource: note\n  version: default\n  importDerivedRoles: [drafts]\n  rules:\n    - actions: [edit]\n      effect: EFFECT_ALLOW\n      derivedRoles: [own_draft]\n      condition:\n        match:\n          expr: R.attr.archived == false\n",
        ))
        .derived_roles(PolicyFile::new(
            "drafts.yaml",
            "apiVersion: auth.flagrante.io/v1\nderivedRoles:\n  name: drafts\n  definitions:\n    - name: own_draft\n      parentRoles: [editor]\n      condition:\n        match:\n          all:\n            of:\n              - expr: R.attr.authorId == P.id\n              - expr: R.attr.status in [\"draft\", \"review\"]\n",
        ))
        .build()
        .expect("loads");
    let rule_set = compile(&policies, &[]);
    assert_eq!(
        rule_set.plan::<Note>(&user(ALICE, &["editor"]), NoteAction::Edit),
        Plan::Where(Filter::All(vec![
            eq("authorId", ALICE),
            Filter::In("status".into(), vec!["draft".into(), "review".into()]),
            eq("archived", false),
        ]))
    );
    assert_eq!(
        rule_set.plan::<Note>(&user(ALICE, &["user"]), NoteAction::Edit),
        Plan::Denied
    );
    assert_eq!(
        policies.catalog(&rule_set).resources[1].scopes,
        Vec::<String>::new()
    );
}

#[test]
fn a_plan_folds_where_no_condition_remains() {
    let mut team = user(ALICE, &["user"]);
    if let Principal::User { attr, .. } = &mut team {
        attr.insert("team".into(), "red".into());
    }
    let by_team = [rule(
        "ALLOW",
        "[view]",
        "[user]",
        Some("R.attr.status == P.attr.team"),
    )];
    let cases = [
        (
            "an unconditional allow",
            vec![rule("ALLOW", "[view]", "[user]", None)],
            user(ALICE, &["user"]),
            Plan::Allowed,
        ),
        (
            "a condition left on the row",
            by_team.to_vec(),
            team,
            Plan::Where(eq("status", "red")),
        ),
        (
            "a principal attribute the principal lacks",
            by_team.to_vec(),
            user(ALICE, &["user"]),
            Plan::Denied,
        ),
        (
            "an unconditional deny beside a conditional allow",
            vec![
                rule("ALLOW", "[view]", "[user]", Some("R.attr.authorId == P.id")),
                rule("DENY", "[view]", "[user]", None),
            ],
            user(ALICE, &["user"]),
            Plan::Denied,
        ),
        (
            "an unconditional allow in one role beside a conditional one in another",
            vec![
                rule("ALLOW", "[view]", "[user]", Some("R.attr.authorId == P.id")),
                rule("ALLOW", "[view]", "[admin]", None),
            ],
            user(ALICE, &["user", "admin"]),
            Plan::Allowed,
        ),
        (
            "an `any` of conditions",
            vec![
                rule("ALLOW", "[view]", "[user]", Some("R.attr.authorId == P.id")),
                rule(
                    "ALLOW",
                    "[view]",
                    "[user]",
                    Some("R.attr.status != \"draft\""),
                ),
            ],
            user(ALICE, &["user"]),
            Plan::Where(Filter::Any(vec![
                eq("authorId", ALICE),
                Filter::Ne("status".into(), "draft".into()),
            ])),
        ),
    ];
    for (case, rules, principal, expected) in cases {
        assert_eq!(
            plan(&rules, &[], &principal, NoteAction::View),
            expected,
            "{case}"
        );
    }
}
