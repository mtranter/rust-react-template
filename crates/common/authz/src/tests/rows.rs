use super::*;
use crate::{Authorizer, Plan, Rows};
use app_error::AppError;

fn authorizer(policies: &Policies) -> Authorizer {
    Authorizer::new(compile(policies, policies.default_grants()))
}

#[derive(Debug, PartialEq, Eq)]
enum Refused {
    NotFound,
    Forbidden,
}

fn refused<T>(result: Result<T, AppError>) -> Result<T, Refused> {
    result.map_err(|e| match e {
        AppError::NotFound(_) => Refused::NotFound,
        AppError::Forbidden => Refused::Forbidden,
        other => panic!("unexpected refusal {other:?}"),
    })
}

fn edit(authorizer: &Authorizer, principal: &Principal, row: &Note) -> Result<(), Refused> {
    refused(
        authorizer
            .authorize::<Note>(principal, NoteAction::Edit)
            .and_then(|plan| authorizer.check(principal, &plan, row)),
    )
}

#[test]
fn another_users_note_is_not_found_to_edit() {
    let authorizer = authorizer(&adr_policies());

    assert_eq!(
        edit(&authorizer, &user(BOB, &["user"]), &note(ALICE, false)),
        Err(Refused::NotFound)
    );
}

#[test]
fn an_authors_archived_note_is_forbidden_to_edit_since_they_may_still_view_it() {
    let authorizer = authorizer(&adr_policies());

    assert_eq!(
        edit(&authorizer, &user(ALICE, &["user"]), &note(ALICE, true)),
        Err(Refused::Forbidden)
    );
    assert_eq!(
        edit(&authorizer, &user(ALICE, &["user"]), &note(ALICE, false)),
        Ok(())
    );
}

#[test]
fn a_public_note_is_forbidden_to_edit_not_hidden_when_a_rule_lets_anyone_view_it() {
    let policies = Policies::builder("notebook")
        .resource::<Note>(note_rules(
            "    - actions: [view]\n      effect: EFFECT_ALLOW\n      roles: [\"*\"]\n      condition:\n        match:\n          expr: R.attr.visibility == \"public\"\n",
        ))
        .derived_roles(scopes())
        .default_grants(defaults())
        .build()
        .expect("the test policy loads");
    let authorizer = authorizer(&policies);
    let bob = user(BOB, &["user"]);
    let public = Note {
        visibility: "public",
        ..note(ALICE, false)
    };

    assert_eq!(edit(&authorizer, &bob, &public), Err(Refused::Forbidden));
    assert_eq!(
        edit(&authorizer, &bob, &note(ALICE, false)),
        Err(Refused::NotFound)
    );
}

#[test]
fn a_list_holds_the_rows_its_plan_allows_and_a_denied_one_is_refused() {
    let authorizer = authorizer(&adr_policies());

    assert_eq!(
        refused(authorizer.rows::<Note>(&user(ALICE, &["user"]), NoteAction::View)),
        Ok(Rows::Where(eq("authorId", ALICE)))
    );
    assert_eq!(
        refused(authorizer.rows::<Note>(&user(BOB, &["admin"]), NoteAction::View)),
        Ok(Rows::All)
    );
    assert_eq!(
        refused(authorizer.rows::<Note>(&user(ALICE, &["guest"]), NoteAction::View)),
        Err(Refused::Forbidden)
    );
    assert_eq!(Plan::Denied.rows(), None);
}
