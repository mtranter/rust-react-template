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
async fn the_catalog_offers_each_resources_actions_operations_and_scopes_named_from_them_unlabelled(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;

    let catalog = access.catalog().await.unwrap();

    assert_eq!(
        *resource(&catalog, "gadget"),
        json!({
            "module": "workshop",
            "moduleLabel": "Workshop",
            "resource": "gadget",
            "label": "Gadget",
            "actions": [
                { "name": "make", "label": "Make", "operations": ["make_gadget"], "reachable": true },
                { "name": "polish", "label": "Polish", "operations": [], "reachable": true },
                { "name": "scrap", "label": "Scrap", "operations": [], "reachable": false },
            ],
            "scopes": [
                { "name": "any", "label": "Any" },
                { "name": "maker", "label": "Maker" },
            ],
        })
    );
    assert!(
        catalog["resources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| !r["resource"].as_str().unwrap().starts_with("module:")),
        "a Module has no gate to offer"
    );
    assert_eq!(resource(&catalog, "grant")["module"], "access");
    assert_eq!(resource(&catalog, "role")["module"], "access");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_catalog_names_a_module_and_its_resources_and_actions_as_its_labels_do(pool: PgPool) {
    let access = Access::seeded(pool).await;

    let catalog = access.catalog().await.unwrap();

    let grant = resource(&catalog, "grant");
    assert_eq!(grant["moduleLabel"], "Access");
    assert_eq!(grant["label"], "Permissions");
    assert_eq!(grant["actions"][1]["name"], "create");
    assert_eq!(grant["actions"][1]["label"], "Grant");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_action_becomes_reachable_once_granted(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
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
