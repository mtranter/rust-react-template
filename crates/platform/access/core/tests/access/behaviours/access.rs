use crate::driver::{Access, Refusal};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn grants_are_refused_to_a_caller_who_is_not_signed_in(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.sign_out();

    assert_eq!(access.grants().await.err(), Some(Refusal::NotSignedIn));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_caller_who_is_not_an_admin_is_kept_out(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let revision = access.revision().await;
    access.sign_in_as("a-user");

    assert_eq!(access.grants().await.err(), Some(Refusal::Forbidden));
    assert_eq!(access.catalog().await.err(), Some(Refusal::Forbidden));
    assert_eq!(access.roles().await.err(), Some(Refusal::Forbidden));
    assert_eq!(
        access.create_role("support", "").await,
        Err(Refusal::Forbidden)
    );
    assert_eq!(
        access.grant("user", "gadget", "scrap", "any").await,
        Err(Refusal::Forbidden)
    );
    assert_eq!(access.revision().await, revision);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_role_granted_access_to_grants_may_list_them(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("auditor", "").await.unwrap();
    access
        .grant("auditor", "module:access", "use", "any")
        .await
        .unwrap();
    access
        .grant("auditor", "grant", "list", "any")
        .await
        .unwrap();
    access.assign("an-auditor", "auditor").await.unwrap();
    access.polls().await;
    access.sign_in_as("an-auditor");

    assert!(access.grants().await.is_ok());
    assert_eq!(
        access.grant("auditor", "grant", "create", "any").await,
        Err(Refusal::Forbidden)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn no_revocation_takes_an_admins_access_to_grants(pool: PgPool) {
    let access = Access::seeded(pool).await;

    for grant in access.grants().await.unwrap().items {
        access.revoke(&grant).await.unwrap();
    }
    access.polls().await;

    assert!(access.grants().await.unwrap().items.is_empty());
    assert!(access.catalog().await.is_ok());
    assert!(access.allows("admin", "module:access", "use"));
    assert!(access.grant("user", "gadget", "make", "any").await.is_ok());
}
