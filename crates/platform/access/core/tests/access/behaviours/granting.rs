use crate::driver::{ADMIN_ID, Access, Refusal};
use access_api::events::v1::AccessGranted;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_grant_takes_effect_at_the_instances_next_poll(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();

    let granted = access
        .grant("support", "gadget", "scrap", "any")
        .await
        .unwrap();

    assert_eq!(
        granted.rule(),
        (
            "support".into(),
            "gadget".into(),
            "scrap".into(),
            "any".into()
        )
    );
    assert_eq!(granted["grantedBy"], ADMIN_ID);
    assert_eq!(granted["stale"], false);
    assert_eq!(granted.version(), 1);
    assert!(!access.allows("support", "gadget", "scrap"));
    access.polls().await;
    assert!(access.allows("support", "gadget", "scrap"));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_grant_is_announced_with_who_made_it(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();

    let granted = access
        .grant("support", "gadget", "make", "maker")
        .await
        .unwrap();

    let announced = access.announced::<AccessGranted>().await;
    assert_eq!(announced.len(), 1);
    let (version, payload) = &announced[0];
    assert_eq!(version.as_i64(), 1);
    assert_eq!(payload.entity.id.to_string(), granted.id());
    assert_eq!(payload.entity.scope, "maker");
    assert_eq!(payload.entity.granted_by.as_deref(), Some(ADMIN_ID));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_grant_the_catalog_lacks_is_refused_naming_the_field_and_changes_nothing(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    let revision = access.revision().await;
    let before = access.grants().await.unwrap().items;

    for (grant, field) in [
        (("support", "gizmo", "make", "any"), "resource"),
        (("support", "gadget", "fly", "any"), "action"),
        (("support", "gadget", "make", "owner"), "scope"),
        (("support", "module:workshop", "use", "any"), "resource"),
        (("  ", "gadget", "make", "any"), "role"),
        (("nobody", "gadget", "make", "any"), "role"),
    ] {
        let (role, resource, action, scope) = grant;
        assert_eq!(
            access.grant(role, resource, action, scope).await,
            Err(Refusal::invalid([field])),
            "{grant:?}"
        );
    }

    assert_eq!(access.grants().await.unwrap().items, before);
    assert_eq!(access.revision().await, revision);
    assert!(access.announced::<AccessGranted>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_grant_equal_to_one_already_made_is_a_conflict_and_changes_nothing(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    access
        .grant("support", "gadget", "make", "any")
        .await
        .unwrap();
    let revision = access.revision().await;
    let announced = access.announced::<AccessGranted>().await.len();

    assert_eq!(
        access.grant("support", "gadget", "make", "any").await,
        Err(Refusal::Conflict)
    );
    assert_eq!(
        access.grant("user", "gadget", "make", "any").await,
        Err(Refusal::Conflict),
        "a default grant counts"
    );

    assert_eq!(access.revision().await, revision);
    assert_eq!(access.announced::<AccessGranted>().await.len(), announced);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_same_action_may_be_granted_within_another_scope(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    access
        .grant("support", "gadget", "make", "any")
        .await
        .unwrap();

    assert!(
        access
            .grant("support", "gadget", "make", "maker")
            .await
            .is_ok()
    );
}
