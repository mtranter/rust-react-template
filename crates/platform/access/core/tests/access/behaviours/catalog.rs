use crate::driver::{Access, CatalogJson};
use serde_json::{Value, json};
use sqlx::PgPool;

fn resource<'a>(catalog: &'a CatalogJson, kind: &str) -> &'a Value {
    catalog["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["resource"] == kind)
        .unwrap_or_else(|| panic!("the catalog offers `{kind}`"))
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_catalog_offers_each_resources_actions_operations_and_scopes(pool: PgPool) {
    let access = Access::seeded(pool).await;

    let catalog = access.catalog().await.unwrap();

    assert_eq!(
        *resource(&catalog, "gadget"),
        json!({
            "module": "workshop",
            "resource": "gadget",
            "actions": [
                { "name": "make", "operations": ["make_gadget"], "reachable": true },
                { "name": "polish", "operations": [], "reachable": true },
                { "name": "scrap", "operations": [], "reachable": false },
            ],
            "scopes": ["any", "maker"],
        })
    );
    assert_eq!(
        *resource(&catalog, "module:workshop"),
        json!({
            "module": "workshop",
            "resource": "module:workshop",
            "actions": [{ "name": "use", "operations": [], "reachable": true }],
            "scopes": ["any"],
        })
    );
    assert_eq!(resource(&catalog, "grant")["module"], "access");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_action_becomes_reachable_once_granted(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access
        .grant("support", "gadget", "scrap", "any")
        .await
        .unwrap();
    access.polls().await;

    let catalog = access.catalog().await.unwrap();

    assert_eq!(
        resource(&catalog, "gadget")["actions"][2]["reachable"],
        true
    );
}
