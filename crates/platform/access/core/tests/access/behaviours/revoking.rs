use crate::driver::{ADMIN_ID, Access, Refusal};
use access_api::events::v1::GrantRevoked;
use sqlx::PgPool;

const NIL_ID: &str = "00000000-0000-0000-0000-000000000000";

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_revoked_grant_stops_allowing_at_the_instances_next_poll(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let making = access.grant_of(("user", "gadget", "make", "any")).await;
    let revision = access.revision().await;

    access.revoke(&making).await.unwrap();

    assert!(access.revision().await > revision);
    assert!(access.allows("user", "gadget", "make"));
    access.polls().await;
    assert!(!access.allows("user", "gadget", "make"));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_revocation_is_announced_with_who_made_it(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let making = access.grant_of(("user", "gadget", "make", "any")).await;

    access.revoke(&making).await.unwrap();

    let announced = access.announced::<GrantRevoked>().await;
    assert_eq!(announced.len(), 1);
    let (version, payload) = &announced[0];
    assert_eq!(version.as_i64(), 2);
    assert_eq!(payload.entity.id.to_string(), making.id());
    assert_eq!(payload.context.revoked_by.as_deref(), Some(ADMIN_ID));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_revocation_needs_the_version_listed(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let making = access.grant_of(("user", "gadget", "make", "any")).await;

    assert_eq!(
        access.revoke_without_precondition(&making).await,
        Err(Refusal::PreconditionRequired)
    );
    assert_eq!(
        access.revoke_at(making.id(), making.version() + 1).await,
        Err(Refusal::Stale)
    );
    assert!(access.announced::<GrantRevoked>().await.is_empty());
    assert_eq!(
        access.grant_of(("user", "gadget", "make", "any")).await,
        making
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn revoking_a_grant_that_is_gone_is_not_found(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let making = access.grant_of(("user", "gadget", "make", "any")).await;
    access.revoke(&making).await.unwrap();

    assert_eq!(access.revoke(&making).await, Err(Refusal::NotFound));
    assert_eq!(access.revoke_at(NIL_ID, 1).await, Err(Refusal::NotFound));
}
