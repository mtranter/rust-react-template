use crate::driver::{Access, Refusal};
use access_api::events::v1::AccessGranted;
use sqlx::PgPool;

fn rules(rules: &[(&str, &str, &str, &str)]) -> Vec<(String, String, String, String)> {
    rules
        .iter()
        .map(|(r, s, a, c)| (r.to_string(), s.to_string(), a.to_string(), c.to_string()))
        .collect()
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn each_resource_is_granted_only_the_actions_and_scopes_it_has(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();

    let done = access
        .grant_in_bulk(
            "support",
            &["gadget", "role"],
            &["make", "create"],
            &["any", "maker"],
        )
        .await
        .unwrap();

    assert_eq!(
        done.granted(),
        rules(&[
            ("support", "gadget", "make", "any"),
            ("support", "gadget", "make", "maker"),
            ("support", "role", "create", "any"),
        ])
    );
    assert!(done.skipped().is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn grants_made_before_are_skipped_not_refused(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    access
        .grant("support", "gadget", "make", "any")
        .await
        .unwrap();

    let done = access
        .grant_in_bulk("support", &["gadget"], &["make", "scrap"], &["any"])
        .await
        .unwrap();

    assert_eq!(
        done.granted(),
        rules(&[("support", "gadget", "scrap", "any")])
    );
    assert_eq!(
        done.skipped(),
        vec![("gadget".into(), "make".into(), "any".into())]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_grants_take_effect_at_the_next_poll_after_one_revision_and_are_each_announced(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    let revision = access.revision().await;

    let done = access
        .grant_in_bulk("support", &["gadget"], &["make", "scrap"], &["any"])
        .await
        .unwrap();

    assert_eq!(access.revision().await, revision + 1);
    let announced = access.announced::<AccessGranted>().await;
    assert_eq!(announced.len(), 2);
    assert_eq!(done.granted().len(), 2);
    assert!(!access.allows("support", "gadget", "scrap"));
    access.polls().await;
    assert!(access.allows("support", "gadget", "scrap"));
    assert!(access.allows("support", "gadget", "make"));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn nothing_new_to_grant_leaves_the_revision_alone(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    access
        .grant("support", "gadget", "make", "any")
        .await
        .unwrap();
    let revision = access.revision().await;

    let done = access
        .grant_in_bulk("support", &["gadget"], &["make"], &["any"])
        .await
        .unwrap();

    assert!(done.granted().is_empty());
    assert_eq!(done.skipped().len(), 1);
    assert_eq!(access.revision().await, revision);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_invalid_bulk_grant_is_refused_naming_the_field_and_changes_nothing(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    let revision = access.revision().await;
    let before = access.grants().await.unwrap().items;

    for (role, resources, actions, scopes, field) in [
        (
            "nobody",
            &["gadget"][..],
            &["make"][..],
            &["any"][..],
            "role",
        ),
        ("  ", &["gadget"], &["make"], &["any"], "role"),
        ("support", &["gizmo"], &["make"], &["any"], "resources"),
        ("support", &[], &["make"], &["any"], "resources"),
        ("support", &["gadget"], &["fly"], &["any"], "actions"),
        ("support", &["gadget"], &[], &["any"], "actions"),
        ("support", &["role"], &["make"], &["any"], "actions"),
        ("support", &["gadget"], &["make"], &["owner"], "scopes"),
        ("support", &["role"], &["create"], &["maker"], "scopes"),
        ("support", &["gadget"], &["make"], &[], "scopes"),
    ] {
        assert_eq!(
            access
                .grant_in_bulk(role, resources, actions, scopes)
                .await
                .err(),
            Some(Refusal::invalid([field])),
            "{role:?} {resources:?} {actions:?} {scopes:?}"
        );
    }

    assert_eq!(access.grants().await.unwrap().items, before);
    assert_eq!(access.revision().await, revision);
    assert!(access.announced::<AccessGranted>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_caller_who_is_not_an_admin_may_not_grant_in_bulk(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    let revision = access.revision().await;
    access.sign_in_as("a-user");

    assert_eq!(
        access
            .grant_in_bulk("support", &["gadget"], &["make"], &["any"])
            .await
            .err(),
        Some(Refusal::Forbidden)
    );
    assert_eq!(access.revision().await, revision);
}
