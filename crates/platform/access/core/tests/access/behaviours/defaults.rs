use crate::driver::Access;
use crate::workshop::{FIRST_RELEASE_DEFAULTS, Release};
use access_api::events::v1::AccessGranted;
use sqlx::PgPool;

fn rules_of(grants: &crate::driver::Grants) -> Vec<(String, String, String, String)> {
    let mut rules: Vec<_> = grants.items.iter().map(|g| g.rule()).collect();
    rules.sort();
    rules
}

fn expected(defaults: &[(&str, &str, &str, &str)]) -> Vec<(String, String, String, String)> {
    let mut rules: Vec<_> = defaults
        .iter()
        .map(|(a, b, c, d)| (a.to_string(), b.to_string(), c.to_string(), d.to_string()))
        .collect();
    rules.sort();
    rules
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_fresh_database_is_granted_every_modules_defaults_by_no_one(pool: PgPool) {
    let access = Access::seeded(pool).await;

    let grants = access.grants().await.unwrap();

    assert_eq!(rules_of(&grants), expected(FIRST_RELEASE_DEFAULTS));
    assert!(grants.items.iter().all(|g| g.get("grantedBy").is_none()));
    assert!(access.allows("user", "gadget", "make"));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn migrating_again_grants_nothing_and_leaves_the_revision(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let revision = access.revision().await;
    let before = access.grants().await.unwrap().items;

    assert_eq!(access.migrate().await, 0);

    assert_eq!(access.grants().await.unwrap().items, before);
    assert_eq!(access.revision().await, revision);
    assert!(access.announced::<AccessGranted>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_default_an_admin_revoked_is_not_granted_again(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let making = access.grant_of(("user", "gadget", "make", "any")).await;
    access.revoke(&making).await.unwrap();

    assert_eq!(access.migrate().await, 0);

    assert!(!rules_of(&access.grants().await.unwrap()).contains(&making.rule()));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_default_added_in_a_later_release_is_granted_by_its_migrate(pool: PgPool) {
    let later = Access::of(pool, Release::Second).await;

    assert_eq!(later.migrate().await, 1);
    later.polls().await;

    let inspecting = later.grant_of(("user", "gadget", "inspect", "any")).await;
    assert!(inspecting.get("grantedBy").is_none());
    assert!(later.allows("user", "gadget", "inspect"));
    assert_eq!(later.announced::<AccessGranted>().await.len(), 1);
}
