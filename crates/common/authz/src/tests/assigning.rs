use crate::{Authorizer, Principal, RuleSet};

fn roles(principal: &Principal) -> Vec<&str> {
    match principal {
        Principal::User { roles, .. } => roles.iter().map(String::as_str).collect(),
        Principal::System => panic!("a subject resolves to a user"),
    }
}

#[test]
fn a_subject_with_nothing_assigned_holds_exactly_user() {
    let authorizer = Authorizer::new(RuleSet::default());

    assert_eq!(roles(&authorizer.principal("ada")), ["user"]);
}

#[test]
fn a_subject_holds_user_and_what_is_assigned_to_it() {
    let authorizer = Authorizer::new(RuleSet::default().assigning([
        ("ada".to_owned(), "support".to_owned()),
        ("ada".to_owned(), "admin".to_owned()),
        ("ada".to_owned(), "user".to_owned()),
        ("bob".to_owned(), "support".to_owned()),
    ]));

    assert_eq!(
        roles(&authorizer.principal("ada")),
        ["user", "admin", "support"]
    );
    assert_eq!(roles(&authorizer.principal("cy")), ["user"]);
}

#[test]
fn a_replaced_rule_set_replaces_the_assignments() {
    let authorizer =
        Authorizer::new(RuleSet::default().assigning([("ada".to_owned(), "support".to_owned())]));

    authorizer.replace(RuleSet::default());

    assert_eq!(roles(&authorizer.principal("ada")), ["user"]);
}

#[test]
fn a_bootstrap_admin_holds_admin_with_nothing_assigned() {
    let authorizer = Authorizer::new(RuleSet::default()).with_bootstrap_admins(["root".to_owned()]);

    assert_eq!(roles(&authorizer.principal("root")), ["user", "admin"]);
    assert!(authorizer.is_bootstrap_admin("root"));
    assert!(!authorizer.is_bootstrap_admin("ada"));

    authorizer.replace(RuleSet::default().assigning([("root".to_owned(), "admin".to_owned())]));
    assert_eq!(roles(&authorizer.principal("root")), ["user", "admin"]);
}

#[test]
fn assigning_in_memory_sets_a_subjects_roles() {
    let authorizer = Authorizer::new(RuleSet::default());

    authorizer.assign_in_memory("ada", &["support"]);
    authorizer.assign_in_memory("ada", &["admin"]);

    assert_eq!(roles(&authorizer.principal("ada")), ["user", "admin"]);
}
