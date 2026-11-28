use crate::driver::{ADMIN_ID, Access, Refusal};
use sqlx::PgPool;

fn summaries(log: &crate::driver::AuditLog) -> Vec<(String, String, String, String)> {
    log.items.iter().map(|e| e.summary()).collect()
}

fn s(change: &str, actor: &str, role: &str, subject: &str) -> (String, String, String, String) {
    (change.into(), actor.into(), role.into(), subject.into())
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn every_change_to_roles_and_assignments_is_logged_with_who_made_it_newest_first(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    access.later(chrono::TimeDelta::seconds(1));
    let assignment = access.assign("someone", "support").await.unwrap();
    access.later(chrono::TimeDelta::seconds(1));
    access.unassign(&assignment).await.unwrap();
    access.later(chrono::TimeDelta::seconds(1));
    let role = access.role("support").await;
    access.delete_role(&role).await.unwrap();

    access.run_pending_events().await;
    let log = access.audit_where(&[("actor", ADMIN_ID)]).await.unwrap();
    assert_eq!(
        summaries(&log),
        [
            s("roleDeleted", ADMIN_ID, "support", ""),
            s("roleUnassigned", ADMIN_ID, "support", "someone"),
            s("roleAssigned", ADMIN_ID, "support", "someone"),
            s("roleCreated", ADMIN_ID, "support", ""),
        ]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_grant_made_and_revoked_is_logged_with_its_resource_action_and_scope(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    access.later(chrono::TimeDelta::seconds(1));
    let grant = access
        .grant("support", "gadget", "make", "any")
        .await
        .unwrap();
    access.later(chrono::TimeDelta::seconds(1));
    access.revoke(&grant).await.unwrap();
    access.run_pending_events().await;

    let log = access
        .audit_where(&[("role", "support"), ("sort", "occurredAt")])
        .await
        .unwrap();

    assert_eq!(
        summaries(&log),
        [
            s("roleCreated", ADMIN_ID, "support", ""),
            s("accessGranted", ADMIN_ID, "support", ""),
            s("grantRevoked", ADMIN_ID, "support", ""),
        ]
    );
    for entry in &log.items[1..] {
        assert_eq!(entry["resource"], "gadget");
        assert_eq!(entry["action"], "make");
        assert_eq!(entry["scope"], "any");
    }
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_log_is_filtered_by_change_subject_actor_and_time(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    access.later(chrono::TimeDelta::hours(1));
    access.assign("someone", "support").await.unwrap();
    access.run_pending_events().await;
    let assigned = access
        .audit_where(&[("change", "roleAssigned"), ("subject", "someone")])
        .await
        .unwrap()
        .items[0]["occurredAt"]
        .as_str()
        .unwrap()
        .to_owned();

    let assignments = access
        .audit_where(&[("change", "roleAssigned"), ("subject", "someone")])
        .await
        .unwrap();
    let since = access
        .audit_where(&[("actor", ADMIN_ID), ("since", &assigned)])
        .await
        .unwrap();
    let by_nobody = access.audit_where(&[("actor", "nobody")]).await.unwrap();

    assert_eq!(
        summaries(&assignments),
        [s("roleAssigned", ADMIN_ID, "support", "someone")]
    );
    assert_eq!(summaries(&since), summaries(&assignments));
    assert_eq!(by_nobody.total, 0);
    assert_eq!(
        access.audit_where(&[("since", "yesterday")]).await.err(),
        Some(Refusal::invalid(["since"]))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_system_s_changes_are_logged_with_no_actor(pool: PgPool) {
    let access = Access::seeded(pool).await;

    access.run_pending_events().await;
    let log = access
        .audit_where(&[("change", "roleAssigned"), ("subject", ADMIN_ID)])
        .await
        .unwrap();

    assert_eq!(summaries(&log), [s("roleAssigned", "", "admin", ADMIN_ID)]);
    assert!(log.items[0].get("actor").is_none());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_log_is_unavailable_until_it_has_caught_up(pool: PgPool) {
    let access = Access::seeded(pool).await;

    assert_eq!(
        access.audit_where(&[]).await.err(),
        Some(Refusal::Unavailable)
    );
    access.run_pending_events().await;
    assert!(access.audit_where(&[]).await.is_ok());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn only_an_admin_reads_the_log(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.run_pending_events().await;

    access.sign_in_as("someone");
    assert_eq!(
        access.audit_where(&[]).await.err(),
        Some(Refusal::Forbidden)
    );
    access.sign_out();
    assert_eq!(
        access.audit_where(&[]).await.err(),
        Some(Refusal::NotSignedIn)
    );
}
