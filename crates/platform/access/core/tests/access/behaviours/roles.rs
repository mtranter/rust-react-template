use crate::driver::{ADMIN_ID, Access, Refusal};
use crate::workshop::MAINTAINER;
use access_api::events::v1::{RoleCreated, RoleDeleted};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_fresh_database_has_the_built_in_roles_which_cant_be_deleted(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let revision = access.revision().await;

    for name in ["admin", "user"] {
        let role = access.role(name).await;
        assert_eq!(role["builtIn"], true);
        assert_eq!(access.delete_role(&role).await, Err(Refusal::Conflict));
    }

    assert_eq!(access.revision().await, revision);
    assert!(access.announced::<RoleDeleted>().await.is_empty());
    assert_eq!(access.role_names().await, ["admin", MAINTAINER, "user"]);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_created_role_is_listed_and_announced_with_who_made_it(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let revision = access.revision().await;

    let created = access
        .create_role(" support ", "Answers customers")
        .await
        .unwrap();

    assert_eq!(created.name(), "support");
    assert_eq!(created["description"], "Answers customers");
    assert_eq!(created["builtIn"], false);
    assert_eq!(created["createdBy"], ADMIN_ID);
    assert_eq!(created.version(), 1);
    assert_eq!(access.role("support").await, created);
    assert!(access.revision().await > revision);
    let announced = access.announced::<RoleCreated>().await;
    assert_eq!(announced.len(), 1);
    let (version, payload) = &announced[0];
    assert_eq!(version.as_i64(), 1);
    assert_eq!(payload.entity.name, "support");
    assert_eq!(payload.entity.created_by.as_deref(), Some(ADMIN_ID));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_role_whose_name_exists_is_a_conflict_and_changes_nothing(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    let revision = access.revision().await;
    let before = access.roles().await.unwrap().items;

    for name in ["support", "admin", MAINTAINER] {
        assert_eq!(
            access.create_role(name, "again").await,
            Err(Refusal::Conflict),
            "{name}"
        );
    }

    assert_eq!(access.roles().await.unwrap().items, before);
    assert_eq!(access.revision().await, revision);
    assert_eq!(access.announced::<RoleCreated>().await.len(), 1);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_role_without_a_name_is_refused_naming_it(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let revision = access.revision().await;

    assert_eq!(
        access.create_role("  ", "").await,
        Err(Refusal::invalid(["name"]))
    );
    assert_eq!(access.revision().await, revision);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_deleted_role_is_gone_and_announced_with_who_deleted_it(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let support = access.create_role("support", "").await.unwrap();
    let revision = access.revision().await;

    access.delete_role(&support).await.unwrap();

    assert!(!access.role_names().await.contains(&"support".to_owned()));
    assert!(access.revision().await > revision);
    let announced = access.announced::<RoleDeleted>().await;
    assert_eq!(announced.len(), 1);
    let (version, payload) = &announced[0];
    assert_eq!(version.as_i64(), 2);
    assert_eq!(payload.entity.name, "support");
    assert_eq!(payload.context.deleted_by.as_deref(), Some(ADMIN_ID));
    assert_eq!(
        access.grant("support", "gadget", "make", "any").await,
        Err(Refusal::invalid(["role"]))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_role_a_grant_names_is_deleted_only_once_the_grant_is_revoked(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let support = access.create_role("support", "").await.unwrap();
    let scrapping = access
        .grant("support", "gadget", "scrap", "any")
        .await
        .unwrap();
    let revision = access.revision().await;

    assert_eq!(access.delete_role(&support).await, Err(Refusal::Conflict));
    assert_eq!(access.revision().await, revision);
    assert!(access.announced::<RoleDeleted>().await.is_empty());

    access.revoke(&scrapping).await.unwrap();
    assert_eq!(access.delete_role(&support).await, Ok(()));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_role_created_again_continues_from_its_deletions_version(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let support = access.create_role("support", "").await.unwrap();
    access.delete_role(&support).await.unwrap();

    let again = access.create_role("support", "").await.unwrap();

    assert_eq!(again.version(), 3);
    let created = access.announced::<RoleCreated>().await;
    assert_eq!(created.last().unwrap().0.as_i64(), 3);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_delete_needs_the_version_listed(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let support = access.create_role("support", "").await.unwrap();

    assert_eq!(
        access.delete_role_without_precondition(&support).await,
        Err(Refusal::PreconditionRequired)
    );
    assert_eq!(
        access
            .delete_role_at("support", support.version() + 1)
            .await,
        Err(Refusal::Stale)
    );
    assert_eq!(access.role("support").await, support);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn deleting_a_role_that_is_gone_is_not_found(pool: PgPool) {
    let access = Access::seeded(pool).await;

    assert_eq!(
        access.delete_role_at("nobody", 1).await,
        Err(Refusal::NotFound)
    );
}
