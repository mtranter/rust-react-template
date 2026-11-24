use super::*;
use crate::{Plan, Policies, Scope};

fn not_archived() -> Filter {
    Filter::None(vec![eq("archived", true)])
}

#[test]
fn adr_0040s_examples_plan_as_it_says() {
    let rule_set = compile(&adr_policies(), adr_policies().default_grants());
    let cases = [
        (
            "user edits",
            user(ALICE, &["user"]),
            NoteAction::Edit,
            Plan::Where(Filter::All(vec![eq("authorId", ALICE), not_archived()])),
        ),
        (
            "user creates",
            user(ALICE, &["user"]),
            NoteAction::Create,
            Plan::Allowed,
        ),
        (
            "user views",
            user(ALICE, &["user"]),
            NoteAction::View,
            Plan::Where(eq("authorId", ALICE)),
        ),
        (
            "user adds an item, never granted",
            user(ALICE, &["user"]),
            NoteAction::AddItem,
            Plan::Denied,
        ),
        (
            "no role",
            user(ALICE, &[]),
            NoteAction::Create,
            Plan::Denied,
        ),
        (
            "an unknown role",
            user(ALICE, &["support"]),
            NoteAction::View,
            Plan::Denied,
        ),
        ("System", Principal::System, NoteAction::Edit, Plan::Allowed),
        (
            "admin edits",
            user(BOB, &["admin"]),
            NoteAction::Edit,
            Plan::Where(not_archived()),
        ),
        (
            "admin views",
            user(BOB, &["admin"]),
            NoteAction::View,
            Plan::Allowed,
        ),
        (
            "admin removes an item",
            user(BOB, &["admin"]),
            NoteAction::RemoveItem,
            Plan::Allowed,
        ),
        (
            "admin and user edit",
            user(BOB, &["user", "admin"]),
            NoteAction::Edit,
            Plan::Where(not_archived()),
        ),
    ];
    for (case, principal, action, expected) in cases {
        assert_eq!(
            rule_set.plan::<Note>(&principal, action),
            expected,
            "{case}"
        );
    }
}

#[test]
fn a_role_granted_nothing_is_denied_every_action_with_no_gate_to_pass() {
    let rule_set = compile(&adr_policies(), adr_policies().default_grants());
    for action in <NoteAction as strum::VariantArray>::VARIANTS {
        assert_eq!(
            rule_set.plan::<Note>(&user(ALICE, &["support"]), *action),
            Plan::Denied,
            "{action:?}"
        );
    }
    assert_eq!(
        rule_set.plan::<Note>(&user(ALICE, &["user"]), NoteAction::Create),
        Plan::Allowed
    );
}

#[test]
fn granting_support_view_turns_it_from_denied_to_allowed() {
    let policies = adr_policies();
    let support = user(ALICE, &["support"]);
    let before = compile(&policies, policies.default_grants());
    assert_eq!(
        before.plan::<Note>(&support, NoteAction::View),
        Plan::Denied
    );

    let mut grants = policies.default_grants().to_vec();
    grants.push(grant("support", "note", "view", Scope::Any));
    let after = compile(&policies, &grants);
    assert_eq!(
        after.plan::<Note>(&support, NoteAction::View),
        Plan::Allowed
    );
    assert_eq!(after.plan::<Note>(&support, NoteAction::Edit), Plan::Denied);
}

#[test]
fn default_grants_expand_to_one_per_action() {
    let policies = adr_policies();
    let admin: Vec<_> = policies
        .default_grants()
        .iter()
        .filter(|g| g.role == "admin" && g.resource == "note")
        .map(|g| g.action.as_str())
        .collect();
    assert_eq!(
        admin,
        [
            "create",
            "list",
            "view",
            "edit",
            "archive",
            "items:add",
            "items:remove"
        ]
    );
    assert!(policies.default_grants().contains(&grant(
        "user",
        "note",
        "edit",
        Scope::Named("author".into())
    )));
    assert!(
        policies
            .default_grants()
            .iter()
            .all(|g| !g.resource.starts_with("module:"))
    );
}

#[test]
fn matches_agrees_with_the_plan() {
    let rule_set = compile(&adr_policies(), adr_policies().default_grants());
    let rows = [
        note(ALICE, false),
        note(ALICE, true),
        note(BOB, false),
        note(BOB, true),
    ];
    let principals = [
        user(ALICE, &["user"]),
        user(BOB, &["admin"]),
        user(ALICE, &["support"]),
        Principal::System,
    ];
    for principal in &principals {
        for action in [NoteAction::View, NoteAction::Edit, NoteAction::Create] {
            let plan = rule_set.plan::<Note>(principal, action);
            for row in &rows {
                let expected = match principal {
                    Principal::System => true,
                    Principal::User { roles, .. } if roles == &["user"] => match action {
                        NoteAction::Create => true,
                        NoteAction::View => row.author_id == ALICE,
                        _ => row.author_id == ALICE && !row.archived,
                    },
                    Principal::User { roles, .. } if roles == &["admin"] => {
                        action != NoteAction::Edit || !row.archived
                    }
                    Principal::User { .. } => false,
                };
                assert_eq!(
                    plan.allows(row),
                    expected,
                    "{principal:?} {action:?} {row:?}: {plan:?}"
                );
                if let Plan::Where(filter) = &plan {
                    assert_eq!(filter.matches(row), expected);
                }
            }
        }
    }
}

#[test]
fn the_catalog_lists_resources_actions_and_scopes_and_marks_the_unreachable() {
    let policies = adr_policies();
    let catalog = policies.catalog(&compile(&policies, policies.default_grants()));
    assert_eq!(catalog.module, "notebook");
    assert_eq!(
        catalog.capability,
        Capability {
            id: "notes".into(),
            label: "Notes".into()
        }
    );
    let kinds: Vec<_> = catalog.resources.iter().map(|r| r.kind.as_str()).collect();
    assert_eq!(kinds, ["note"]);
    let note = &catalog.resources[0];
    let scopes: Vec<_> = note.scopes.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(scopes, ["any", "author"]);
    assert!(note.actions.iter().all(|a| a.reachable));

    let only_users = compile(
        &policies,
        &policies
            .default_grants()
            .iter()
            .filter(|g| g.role == "user")
            .cloned()
            .collect::<Vec<_>>(),
    );
    let unreachable: Vec<_> = policies.catalog(&only_users).resources[0]
        .actions
        .iter()
        .filter(|a| !a.reachable)
        .map(|a| a.name.clone())
        .collect();
    assert_eq!(unreachable, ["items:add", "items:remove"]);

    let none = Policies::catalog(&policies, &compile(&policies, &[]));
    assert!(
        none.resources
            .iter()
            .flat_map(|r| &r.actions)
            .all(|a| !a.reachable)
    );
}

#[test]
fn the_catalog_labels_what_its_labels_file_names_and_words_the_rest() {
    let policies = Policies::builder("notebook")
        .capability(crate::tests::capability())
        .resource::<Note>(note_policy())
        .derived_roles(scopes())
        .default_grants(defaults())
        .labels(PolicyFile::new("labels.yaml", include_str!("labels.yaml")))
        .build()
        .expect("loads");
    let catalog = policies.catalog(&compile(&policies, &[]));
    assert_eq!(catalog.label, "Notebook");
    let note = &catalog.resources[0];
    assert_eq!(note.label, "Notes");
    let actions: Vec<_> = note
        .actions
        .iter()
        .map(|a| (a.name.as_str(), a.label.as_str()))
        .collect();
    assert!(actions.contains(&("create", "Write")));
    assert!(actions.contains(&("items:add", "Add items")));
    assert!(actions.contains(&("items:remove", "Items remove")));
    let scopes: Vec<_> = note
        .scopes
        .iter()
        .map(|s| (s.name.as_str(), s.label.as_str()))
        .collect();
    assert_eq!(
        scopes,
        [("any", "Every note"), ("author", "Notes they wrote")]
    );

    let unlabelled = adr_policies().catalog(&compile(&policies, &[]));
    assert_eq!(unlabelled.label, "Notebook");
    assert_eq!(unlabelled.resources[0].label, "Note");
    assert_eq!(unlabelled.resources[0].scopes[0].label, "Any");
}
