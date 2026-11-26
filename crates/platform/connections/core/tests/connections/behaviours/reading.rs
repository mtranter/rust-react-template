use crate::driver::{BILLING, CRM, Connections, ERP, FEED, LEDGER, Refusal, SEARCH};
use app_test_support::assert_json_matches;
use serde_json::json;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn every_declared_connection_is_listed_by_key_before_an_admin_configures_it(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    assert_eq!(
        connections.keys().await,
        [BILLING, CRM, ERP, FEED, LEDGER, SEARCH]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_connection_reads_as_declared_with_no_values_yet(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    let crm = connections.get(CRM).await.unwrap();

    assert_eq!(
        *crm,
        json!({
            "key": "crm",
            "label": "CRM",
            "auth": { "style": "api_key_header", "name": "X-Api-Key", "scopes": [] },
            "url": null,
            "fields": [{ "name": "tenant", "label": "Tenant", "value": null }],
            "secrets": [{
                "name": "api_key",
                "label": "API key",
                "set": false,
                "setAt": null,
                "setBy": null,
            }],
            "healthCheck": { "method": "GET", "path": "/health" },
            "status": "not_configured",
            "lastError": null,
            "testedAt": null,
            "version": "1",
        })
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn each_auth_style_reads_with_the_fields_and_secrets_it_takes(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    let erp = connections.get(ERP).await.unwrap();
    assert_json_matches!(erp, "$.auth.style", "oauth2_client_credentials");
    assert_json_matches!(erp, "$.auth.scopes[0]", "orders.read");
    assert_json_matches!(erp, "$.fields[0].name", "token_url");
    assert_json_matches!(erp, "$.fields[1].name", "client_id");
    assert_json_matches!(erp, "$.secrets[0].name", "client_secret");

    let ledger = connections.get(LEDGER).await.unwrap();
    assert_json_matches!(ledger, "$.fields[0].name", "username");
    assert_json_matches!(ledger, "$.secrets[0].name", "password");

    let feed = connections.get(FEED).await.unwrap();
    assert_json_matches!(feed, "$.auth.style", "none");
    assert_eq!(feed["secrets"], json!([]));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_key_no_module_declares_is_not_found(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    assert_eq!(connections.get("nowhere").await, Err(Refusal::NotFound));
    assert_eq!(connections.get("NOT_A_KEY").await, Err(Refusal::NotFound));
}
