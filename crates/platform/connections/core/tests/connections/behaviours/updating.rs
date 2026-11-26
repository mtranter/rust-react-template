use crate::driver::{CRM, Connections, Refusal};
use app_events::EventPayload;
use app_persistence::Version;
use app_test_support::assert_json_matches;
use connections_api::events::v1::{self, ConnectionUpdated};
use serde_json::json;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_admin_sets_a_connections_url_and_fields_and_reads_them_back(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    let updated = connections
        .update(
            CRM,
            1,
            json!({ "url": "https://crm.example/api", "fields": { "tenant": " acme " } }),
        )
        .await
        .unwrap();

    assert_json_matches!(updated, "$.url", "https://crm.example/api");
    assert_eq!(updated.field("tenant"), "acme");
    assert_json_matches!(updated, "$.version", "2");
    assert_eq!(connections.get(CRM).await, Ok(updated));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_update_leaves_what_it_does_not_name_and_clears_what_it_empties(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    connections
        .update(
            CRM,
            1,
            json!({ "url": "https://crm.example", "fields": { "tenant": "acme" } }),
        )
        .await
        .unwrap();

    let kept = connections
        .update(CRM, 2, json!({ "fields": {} }))
        .await
        .unwrap();
    assert_json_matches!(kept, "$.url", "https://crm.example");
    assert_eq!(kept.field("tenant"), "acme");

    let cleared = connections
        .update(CRM, 3, json!({ "url": null, "fields": { "tenant": "" } }))
        .await
        .unwrap();
    assert!(cleared["url"].is_null());
    assert!(cleared.field("tenant").is_null());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn updating_a_connection_announces_it_as_it_now_is(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    connections
        .set_url(CRM, 1, "https://crm.example")
        .await
        .unwrap();

    assert_eq!(
        connections.announced::<ConnectionUpdated>().await,
        [(
            Version::from_i64(2),
            EventPayload::new(v1::Connection {
                key: CRM.into(),
                label: "CRM".into(),
                declared: true,
                url: Some("https://crm.example".into()),
                fields: Default::default(),
                secrets: Vec::new(),
            })
        )]
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_url_that_is_not_http_is_rejected_and_changes_nothing(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    let before = connections.get(CRM).await.unwrap();

    for url in ["ftp://crm.example", "crm.example"] {
        assert_eq!(
            connections.set_url(CRM, 1, url).await,
            Err(Refusal::invalid(["url"]))
        );
    }
    assert_eq!(connections.get(CRM).await, Ok(before));
    assert!(
        connections
            .announced::<ConnectionUpdated>()
            .await
            .is_empty()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_field_the_connection_does_not_declare_is_rejected(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    assert_eq!(
        connections
            .update(CRM, 1, json!({ "fields": { "region": "eu" } }))
            .await,
        Err(Refusal::invalid(["fields.region"]))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_update_against_a_stale_version_is_refused_and_changes_nothing(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    let before = connections.get(CRM).await.unwrap();

    assert_eq!(
        connections.set_url(CRM, 7, "https://crm.example").await,
        Err(Refusal::Stale)
    );
    assert_eq!(connections.get(CRM).await, Ok(before));
}
