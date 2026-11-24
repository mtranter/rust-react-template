use crate::driver::{Access, Refusal};
use sqlx::PgPool;

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(r, a)| ((*r).to_owned(), (*a).to_owned()))
        .collect()
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_may_do_what_the_default_grants_give_every_user_scoped_or_not(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.sign_in_as("a-user");

    assert_eq!(
        access.my_permissions().await,
        Ok(pairs(&[("gadget", "make"), ("gadget", "polish")]))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_role_granted_an_action_within_a_scope_may_do_it(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("scrapper", "").await.unwrap();
    access
        .grant("scrapper", "gadget", "scrap", "maker")
        .await
        .unwrap();
    access.assign("a-scrapper", "scrapper").await.unwrap();
    access.polls().await;
    access.sign_in_as("a-scrapper");

    assert_eq!(
        access.my_permissions().await,
        Ok(pairs(&[
            ("gadget", "make"),
            ("gadget", "polish"),
            ("gadget", "scrap"),
        ]))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_admin_may_do_what_the_built_in_rules_allow_with_no_grant_of_it(pool: PgPool) {
    let access = Access::seeded(pool).await;
    for resource in ["subject", "role", "grant"] {
        assert!(
            access
                .grants_where(&[("role", "admin"), ("resource", resource)])
                .await
                .unwrap()
                .items
                .is_empty(),
            "admin holds no grant on `{resource}`"
        );
    }

    let permissions = access.my_permissions().await.unwrap();

    for needed in pairs(&[("subject", "list"), ("role", "list"), ("grant", "list")]) {
        assert!(
            permissions.contains(&needed),
            "{needed:?} in {permissions:?}"
        );
    }
    let mut sorted = permissions.clone();
    sorted.sort();
    assert_eq!(permissions, sorted);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_revoked_grant_is_no_longer_among_the_callers_permissions(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let grant = access.grant_of(("user", "gadget", "make", "any")).await;
    access.revoke(&grant).await.unwrap();
    access.polls().await;
    access.sign_in_as("a-user");

    assert_eq!(
        access.my_permissions().await,
        Ok(pairs(&[("gadget", "polish")]))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_caller_who_is_not_signed_in_is_told_nothing(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.sign_out();

    assert_eq!(access.my_permissions().await, Err(Refusal::NotSignedIn));
}
