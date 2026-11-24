use crate::driver::{Access, CatalogJson};
use crate::workshop::FIRST_RELEASE_DEFAULTS;
use access_api::events::v1::GrantRevoked;
use app_authz::Plan;
use sqlx::PgPool;

type Rule = (String, String, String, String);

fn rule(role: &str, resource: &str, action: &str, scope: &str) -> Rule {
    (role.into(), resource.into(), action.into(), scope.into())
}

fn first_release_defaults() -> Vec<Rule> {
    let mut rules: Vec<_> = FIRST_RELEASE_DEFAULTS
        .iter()
        .map(|(a, b, c, d)| rule(a, b, c, d))
        .collect();
    rules.sort();
    rules
}

/// The gates every role held under the first release's defaults, before ADR 0047.
const FIRST_RELEASE_GATES: &[(&str, &str)] = &[("user", "workshop"), ("admin", "workshop")];

/// `(module, resource, action)` for everything the catalog offers.
fn offered(catalog: &CatalogJson) -> Vec<(String, String, String)> {
    catalog["resources"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| {
            r["actions"].as_array().unwrap().iter().map(|a| {
                (
                    r["module"].as_str().unwrap().to_owned(),
                    r["resource"].as_str().unwrap().to_owned(),
                    a["name"].as_str().unwrap().to_owned(),
                )
            })
        })
        .collect()
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_role_granted_a_resource_without_its_gate_loses_that_grant(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    access
        .grant("support", "gadget", "scrap", "any")
        .await
        .unwrap();
    access.had_gates(&[("admin", "workshop")]).await;

    access.migrate().await;

    let stored = access.stored_grants().await;
    assert!(!stored.contains(&rule("support", "gadget", "scrap", "any")));
    assert!(!stored.contains(&rule("user", "gadget", "make", "any")));
    assert!(stored.contains(&rule("admin", "gadget", "make", "any")));
    let revoked: Vec<_> = access
        .announced::<GrantRevoked>()
        .await
        .into_iter()
        .map(|(_, e)| (e.entity.role, e.entity.resource, e.context.revoked_by))
        .collect();
    assert!(revoked.contains(&("support".into(), "gadget".into(), None)));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_role_granted_a_resource_and_its_gate_keeps_the_resource_grant(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    access
        .grant("support", "gadget", "scrap", "any")
        .await
        .unwrap();
    access
        .had_gates(&[("admin", "workshop"), ("support", "workshop")])
        .await;

    access.migrate().await;
    access.polls().await;

    assert!(
        access
            .stored_grants()
            .await
            .contains(&rule("support", "gadget", "scrap", "any"))
    );
    assert!(access.allows("support", "gadget", "scrap"));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn every_gate_grant_is_revoked_by_no_one_and_announced_and_the_rules_rebuild_once(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;
    access.had_gates(FIRST_RELEASE_GATES).await;
    let revision = access.revision().await;

    access.migrate().await;

    let stored = access.stored_grants().await;
    assert!(stored.iter().all(|(_, r, _, _)| !r.starts_with("module:")));
    assert_eq!(stored, first_release_defaults());
    let revoked = access.announced::<GrantRevoked>().await;
    assert_eq!(revoked.len(), FIRST_RELEASE_GATES.len());
    for (version, event) in &revoked {
        assert_eq!(version.as_i64(), 2);
        assert!(event.entity.resource.starts_with("module:"));
        assert_eq!(event.context.revoked_by, None);
    }
    assert_eq!(access.revision().await, revision + 1);
    assert!(access.gates_retired().await);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn migrating_again_once_the_gates_are_retired_changes_nothing(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    access.had_gates(&[("admin", "workshop")]).await;
    access.migrate().await;
    let stored = access.stored_grants().await;
    let revision = access.revision().await;
    let announced = access.announced::<GrantRevoked>().await.len();
    access
        .grant("support", "gadget", "scrap", "any")
        .await
        .unwrap();

    assert_eq!(access.migrate().await, 0);

    let mut expected = stored;
    expected.push(rule("support", "gadget", "scrap", "any"));
    expected.sort();
    assert_eq!(access.stored_grants().await, expected);
    assert_eq!(
        access.revision().await,
        revision + 1,
        "the grant's bump alone"
    );
    assert_eq!(access.announced::<GrantRevoked>().await.len(), announced);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_fresh_database_revokes_nothing_and_is_granted_the_defaults_as_before(pool: PgPool) {
    let access = Access::seeded(pool).await;

    assert!(access.gates_retired().await);
    assert!(access.announced_ever::<GrantRevoked>().await.is_empty());
    assert_eq!(access.stored_grants().await, first_release_defaults());
}

/// The database as the first release seeded it before ADR 0047, with roles granted with and
/// without their resources' gates: each may do afterwards exactly what it could before, when a
/// `Caller` passed only roles some role of theirs (`user` included) let through the gate.
#[app_test_db::test(crate::driver::TEMPLATE)]
async fn every_role_may_do_after_retiring_the_gates_exactly_what_it_could_before(pool: PgPool) {
    let access = Access::seeded(pool).await;
    for (role, resource, action) in [
        ("support", "gadget", "scrap"),
        ("auditor", "grant", "list"),
        ("inspector", "grant", "list"),
        ("inspector", "subject", "list"),
    ] {
        let _ = access.create_role(role, "").await;
        access.grant(role, resource, action, "any").await.unwrap();
    }
    let mut gates = FIRST_RELEASE_GATES.to_vec();
    gates.push(("inspector", "access"));
    access.had_gates(&gates).await;
    access.polls().await;
    let offered = offered(&access.catalog().await.unwrap());
    let roles = access.role_names().await;
    let passes = |role: &str, module: &str| {
        (module == "access" && role == "admin")
            || gates
                .iter()
                .any(|(holder, m)| *m == module && (*holder == role || *holder == "user"))
    };
    let could = |access: &Access, gated: bool| {
        roles
            .iter()
            .flat_map(|role| {
                offered.iter().map(move |(module, resource, action)| {
                    let plan = if gated && !passes(role, module) {
                        Plan::Denied
                    } else {
                        access.plan(role, resource, action)
                    };
                    ((role.clone(), resource.clone(), action.clone()), plan)
                })
            })
            .collect::<Vec<_>>()
    };
    let before = could(&access, true);

    access.migrate().await;
    access.polls().await;

    assert_eq!(could(&access, false), before);
    assert!(
        !access
            .stored_grants()
            .await
            .contains(&rule("auditor", "grant", "list", "any"))
    );
}
