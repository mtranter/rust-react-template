use crate::driver::{BILLING, CRM, Connections, MASTER_KEY, Refusal, SUBJECT};
use app_test_support::{assert_json_matches, assert_json_not_exists};
use connections::KeyError;
use connections_api::events::v1::{ConnectionSecretCleared, ConnectionSecretSet, OfSecret};
use serde_json::json;
use sqlx::PgPool;

const API_KEY: &str = "sk_live_0123456789abcdef";

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_set_secret_reads_back_as_set_by_whom_and_when_but_never_its_value(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    let set = connections
        .set_secret(CRM, "api_key", 1, API_KEY)
        .await
        .unwrap();

    let read = connections.get(CRM).await.unwrap();
    assert_eq!(read, set);
    assert_eq!(
        read.secret("api_key"),
        &json!({
            "name": "api_key",
            "label": "API key",
            "set": true,
            "setAt": "2025-01-02T00:00:00+00:00",
            "setBy": SUBJECT,
        })
    );
    assert_json_matches!(read, "$.version", "2");
    assert_json_not_exists!(read, "$.secrets[0].value");
    assert!(!read.to_string().contains(API_KEY));
    assert!(
        !connections
            .list()
            .await
            .unwrap()
            .items
            .iter()
            .any(|c| c.to_string().contains(API_KEY))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn setting_a_secret_announces_which_one_but_not_its_value(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    connections
        .set_secret(CRM, "api_key", 1, API_KEY)
        .await
        .unwrap();

    let announced = connections.announced::<ConnectionSecretSet>().await;
    assert_eq!(announced.len(), 1);
    let (version, payload) = &announced[0];
    assert_eq!(version.as_i64(), 2);
    assert_eq!(
        payload.context,
        OfSecret {
            name: "api_key".into()
        }
    );
    assert_eq!(payload.entity.secrets[0].set_by, SUBJECT);
    assert!(!connections.outbox().await.contains(API_KEY));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_stored_secret_is_sealed_and_opens_only_with_the_key_that_sealed_it(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    connections
        .set_secret(CRM, "api_key", 1, API_KEY)
        .await
        .unwrap();

    let stored = connections.stored_secret(CRM, "api_key").await;
    for bytes in [&stored.ciphertext, &stored.wrapped_key] {
        assert!(
            !bytes
                .windows(API_KEY.len())
                .any(|window| window == API_KEY.as_bytes())
        );
    }
    assert!(stored.key_version.starts_with("env:"));
    assert_eq!(
        connections
            .unseal(CRM, "api_key", MASTER_KEY)
            .await
            .unwrap(),
        API_KEY
    );
    assert!(matches!(
        connections.unseal(CRM, "api_key", [7; 32]).await,
        Err(KeyError::WrongKey)
    ));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn setting_a_secret_again_replaces_it(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    connections
        .set_secret(CRM, "api_key", 1, "first")
        .await
        .unwrap();

    connections
        .set_secret(CRM, "api_key", 2, "second")
        .await
        .unwrap();

    assert_eq!(
        connections
            .unseal(CRM, "api_key", MASTER_KEY)
            .await
            .unwrap(),
        "second"
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn without_a_master_key_a_secret_is_not_set_and_nothing_changes(pool: PgPool) {
    let connections = Connections::without_a_master_key(pool).await;
    let before = connections.get(CRM).await.unwrap();

    assert_eq!(
        connections.set_secret(CRM, "api_key", 1, API_KEY).await,
        Err(Refusal::NotConfigured)
    );
    assert_eq!(connections.get(CRM).await, Ok(before));
    assert!(
        connections
            .announced::<ConnectionSecretSet>()
            .await
            .is_empty()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn without_a_master_key_the_rest_still_works(pool: PgPool) {
    let connections = Connections::without_a_master_key(pool).await;

    assert!(
        connections
            .set_url(CRM, 1, "https://crm.example")
            .await
            .is_ok()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_secret_the_auth_style_does_not_take_is_not_found(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    assert_eq!(
        connections.set_secret(CRM, "token", 1, API_KEY).await,
        Err(Refusal::NotFound)
    );
    assert_eq!(
        connections.clear_secret(BILLING, "api_key", 1).await,
        Err(Refusal::NotFound)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_empty_secret_is_rejected(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    assert_eq!(
        connections.set_secret(CRM, "api_key", 1, "").await,
        Err(Refusal::invalid(["value"]))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_secret_is_set_only_against_the_connections_current_version(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    assert_eq!(
        connections.set_secret(CRM, "api_key", 3, API_KEY).await,
        Err(Refusal::Stale)
    );
    assert_eq!(
        connections
            .set_secret_without_precondition(CRM, "api_key", API_KEY)
            .await,
        Err(Refusal::PreconditionRequired)
    );
    assert!(
        connections
            .announced::<ConnectionSecretSet>()
            .await
            .is_empty()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn clearing_a_secret_unsets_it_and_announces_which_one(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    connections
        .set_secret(BILLING, "token", 1, "tok")
        .await
        .unwrap();

    let cleared = connections.clear_secret(BILLING, "token", 2).await.unwrap();

    assert_eq!(cleared.secret("token")["set"], false);
    assert_json_matches!(cleared, "$.version", "3");
    let announced = connections.announced::<ConnectionSecretCleared>().await;
    assert_eq!(announced.len(), 1);
    assert_eq!(announced[0].1.context.name, "token");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn clearing_a_secret_that_is_not_set_changes_nothing(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    let unchanged = connections.clear_secret(BILLING, "token", 1).await.unwrap();

    assert_json_matches!(unchanged, "$.version", "1");
    assert!(
        connections
            .announced::<ConnectionSecretCleared>()
            .await
            .is_empty()
    );
}
