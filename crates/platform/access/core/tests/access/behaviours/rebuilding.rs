use crate::driver::Access;
use crate::workshop::Release;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_grant_of_an_action_a_later_release_removed_is_listed_stale_and_skipped(pool: PgPool) {
    let first = Access::seeded(pool.clone()).await;
    first
        .grant("support", "gadget", "polish", "any")
        .await
        .unwrap();

    // Serving at all means its first rebuild, stale grants and all, succeeded.
    let later = Access::of(pool, Release::Second).await;

    let polishing = later.grant_of(("support", "gadget", "polish", "any")).await;
    assert_eq!(polishing["stale"], true);
    let making = later.grant_of(("user", "gadget", "make", "any")).await;
    assert_eq!(making["stale"], false);
    assert!(later.allows("user", "gadget", "make"));
    assert!(!later.allows("support", "gadget", "polish"));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_poll_with_nothing_changed_keeps_the_rule_set(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let revision = access.revision().await;

    access.polls().await;

    assert_eq!(access.revision().await, revision);
    assert!(access.allows("user", "gadget", "make"));
}
