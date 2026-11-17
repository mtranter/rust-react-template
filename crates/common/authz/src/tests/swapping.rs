use super::*;
use crate::{Authorizer, Plan, Scope};

#[test]
fn a_replaced_rule_set_is_not_seen_by_a_plan_already_under_way() {
    let policies = adr_policies();
    let support = user(ALICE, &["support"]);
    let authorizer = Authorizer::new(compile(&policies, policies.default_grants()));

    let snapshot = authorizer.current();
    let mut grants = policies.default_grants().to_vec();
    grants.push(grant("support", "note", "view", Scope::Any));
    authorizer.replace(compile(&policies, &grants));

    assert_eq!(
        snapshot.plan::<Note>(&support, NoteAction::View),
        Plan::Denied
    );
    assert_eq!(
        authorizer.plan::<Note>(&support, NoteAction::View),
        Plan::Allowed
    );
    assert_eq!(
        authorizer
            .current()
            .plan::<Note>(&support, NoteAction::View),
        Plan::Allowed
    );
}
