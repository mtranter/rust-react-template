use crate::driver::{ADMIN, ADMIN_ID, Access, Refusal};
use crate::workshop::Release;
use access_api::events::v1::{RoleAssigned, RoleUnassigned};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_subject_with_nothing_assigned_holds_exactly_user(pool: PgPool) {
    let access = Access::seeded(pool).await;

    assert_eq!(access.roles_of("someone"), ["user"]);
    assert_eq!(access.roles_of(ADMIN_ID), ["user", "admin"]);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_assigned_role_is_held_from_the_next_poll_and_announced_with_who_assigned_it(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    let revision = access.revision().await;

    let assigned = access.assign(" ada ", "support").await.unwrap();

    assert_eq!(assigned["subject"], "ada");
    assert_eq!(assigned["role"], "support");
    assert_eq!(assigned["assignedBy"], ADMIN_ID);
    assert_eq!(assigned.version(), 1);
    assert!(access.revision().await > revision);
    assert_eq!(access.roles_of("ada"), ["user"], "not until the next poll");
    access.polls().await;
    assert_eq!(access.roles_of("ada"), ["user", "support"]);
    let announced = access.announced::<RoleAssigned>().await;
    assert_eq!(announced.len(), 1);
    let (version, payload) = &announced[0];
    assert_eq!(version.as_i64(), 1);
    assert_eq!(
        (
            payload.entity.subject.as_str(),
            payload.entity.role.as_str()
        ),
        ("ada", "support")
    );
    assert_eq!(payload.entity.assigned_by.as_deref(), Some(ADMIN_ID));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_unassigned_role_is_dropped_at_the_next_poll_and_assigning_it_again_continues_its_versions(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    let assigned = access.assign("ada", "support").await.unwrap();
    access.polls().await;
    let revision = access.revision().await;

    access.unassign(&assigned).await.unwrap();

    assert!(access.revision().await > revision);
    access.polls().await;
    assert_eq!(access.roles_of("ada"), ["user"]);
    let announced = access.announced::<RoleUnassigned>().await;
    assert_eq!(announced.len(), 1);
    let (version, payload) = &announced[0];
    assert_eq!(version.as_i64(), 2);
    assert_eq!(payload.context.unassigned_by.as_deref(), Some(ADMIN_ID));
    assert_eq!(access.assign("ada", "support").await.unwrap().version(), 3);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn user_is_never_assigned_or_unassigned_and_the_refusal_changes_nothing(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let revision = access.revision().await;

    assert_eq!(
        access.assign("ada", "user").await,
        Err(Refusal::invalid(["role"]))
    );
    assert_eq!(
        access.unassign_at("ada", "user", 1).await,
        Err(Refusal::invalid(["role"]))
    );

    assert_eq!(access.revision().await, revision);
    assert!(access.announced::<RoleAssigned>().await.is_empty());
    assert!(access.announced::<RoleUnassigned>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_role_that_doesnt_exist_is_refused_naming_it_and_changes_nothing(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let revision = access.revision().await;

    assert_eq!(
        access.assign("ada", "suport").await,
        Err(Refusal::invalid(["role"]))
    );
    assert_eq!(
        access.assign(" ", "admin").await,
        Err(Refusal::invalid(["subject"]))
    );

    assert_eq!(access.revision().await, revision);
    assert!(access.announced::<RoleAssigned>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_role_the_subject_holds_already_is_a_conflict(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let revision = access.revision().await;

    assert_eq!(access.assign(ADMIN_ID, ADMIN).await, Err(Refusal::Conflict));
    assert_eq!(access.revision().await, revision);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_role_an_assignment_names_cant_be_deleted_until_it_is_unassigned(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    let assigned = access.assign("ada", "support").await.unwrap();
    let revision = access.revision().await;

    let support = access.role("support").await;
    assert_eq!(access.delete_role(&support).await, Err(Refusal::Conflict));
    assert_eq!(access.revision().await, revision);

    access.unassign(&assigned).await.unwrap();
    assert_eq!(access.delete_role(&support).await, Ok(()));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn unassigning_needs_the_current_version_of_an_assignment_that_exists(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    access.assign("ada", "support").await.unwrap();
    let revision = access.revision().await;

    assert_eq!(
        access.unassign_at("ada", "support", 2).await,
        Err(Refusal::Stale)
    );
    assert_eq!(
        access.unassign_without_precondition("ada", "support").await,
        Err(Refusal::PreconditionRequired)
    );
    assert_eq!(
        access.unassign_at("bob", "support", 1).await,
        Err(Refusal::NotFound)
    );
    assert_eq!(access.revision().await, revision);
    assert!(access.announced::<RoleUnassigned>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_last_assigned_admin_cant_be_unassigned_without_bootstrap_admins(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let revision = access.revision().await;

    assert_eq!(
        access.unassign_at(ADMIN_ID, ADMIN, 1).await,
        Err(Refusal::Conflict)
    );
    assert_eq!(access.revision().await, revision);

    let other = access.assign("another-admin", ADMIN).await.unwrap();
    assert_eq!(access.unassign_at(ADMIN_ID, ADMIN, 1).await, Ok(()));
    access.sign_in_as("another-admin");
    access.polls().await;
    assert_eq!(access.unassign(&other).await, Err(Refusal::Conflict));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_bootstrap_admin_holds_admin_with_nothing_assigned_and_cant_be_unassigned_it(
    pool: PgPool,
) {
    let access = Access::with_bootstrap_admins(pool, Release::First, &["root"]).await;
    access.sign_in_as("root");

    assert_eq!(access.roles_of("root"), ["user", "admin"]);
    assert!(access.grants().await.is_ok());
    assert_eq!(
        access.unassign_at("root", ADMIN, 1).await,
        Err(Refusal::Conflict)
    );
    assert_eq!(
        access.unassign_at(ADMIN_ID, ADMIN, 1).await,
        Ok(()),
        "with a bootstrap admin, the last assigned admin may go"
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_caller_who_is_not_an_admin_assigns_nothing(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let revision = access.revision().await;
    access.sign_in_as("a-user");

    assert_eq!(
        access.assign("a-user", ADMIN).await,
        Err(Refusal::Forbidden)
    );
    assert_eq!(
        access.unassign_at(ADMIN_ID, ADMIN, 1).await,
        Err(Refusal::Forbidden)
    );
    assert_eq!(access.revision().await, revision);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn resolving_a_principals_roles_reads_no_database(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.assign("ada", ADMIN).await.unwrap();
    access.polls().await;
    access.sign_in_as("ada");

    access.database_goes_away().await;

    assert!(
        access.catalog().await.is_ok(),
        "the catalog reads nothing stored, so only resolving `ada` could, and it doesn't"
    );
}
