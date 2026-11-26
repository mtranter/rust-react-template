use crate::driver::{CRM, Connections, FEED, Refusal, crm, declarations};
use app_test_support::assert_json_matches;
use connections_api::events::v1::{ConnectionDeclared, ConnectionRedeclared, ConnectionRetired};
use connections_api::{Auth, Declaration};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn declaring_what_is_already_recorded_changes_nothing(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    assert_eq!(connections.declare(declarations()).await, 0);
    assert!(
        connections
            .announced::<ConnectionRedeclared>()
            .await
            .is_empty()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_newly_declared_connection_is_recorded_and_announced(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    let mut declared = declarations();
    declared.push(Declaration::new("maps", "Maps").auth(Auth::Bearer));

    assert_eq!(connections.declare(declared).await, 1);

    assert_json_matches!(connections.get("maps").await.unwrap(), "$.label", "Maps");
    let announced = connections.announced::<ConnectionDeclared>().await;
    assert_eq!(announced.len(), 1);
    assert_eq!(announced[0].1.entity.key, "maps");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_changed_declaration_keeps_the_values_it_still_names(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    connections
        .set_url(CRM, 1, "https://crm.example")
        .await
        .unwrap();
    connections
        .set_secret(CRM, "api_key", 2, "sk")
        .await
        .unwrap();
    let relabelled: Vec<_> = declarations()
        .into_iter()
        .map(|d| {
            if d.key == CRM {
                let mut relabelled = crm();
                relabelled.label = "Customers".into();
                relabelled
            } else {
                d
            }
        })
        .collect();

    assert_eq!(connections.declare(relabelled).await, 1);

    let read = connections.get(CRM).await.unwrap();
    assert_json_matches!(read, "$.label", "Customers");
    assert_json_matches!(read, "$.url", "https://crm.example");
    assert_eq!(read.secret("api_key")["set"], true);
    assert_eq!(
        connections.announced::<ConnectionRedeclared>().await.len(),
        1
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_connection_no_module_declares_any_more_is_retired_until_declared_again(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    connections
        .set_url(FEED, 1, "https://feed.example")
        .await
        .unwrap();
    let without_feed: Vec<_> = declarations()
        .into_iter()
        .filter(|d| d.key != FEED)
        .collect();

    assert_eq!(connections.declare(without_feed).await, 1);
    assert_eq!(connections.get(FEED).await, Err(Refusal::NotFound));
    assert!(!connections.keys().await.contains(&FEED.to_owned()));
    assert_eq!(connections.announced::<ConnectionRetired>().await.len(), 1);

    connections.declare(declarations()).await;
    assert_json_matches!(
        connections.get(FEED).await.unwrap(),
        "$.url",
        "https://feed.example"
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_key_two_modules_declare_is_refused_and_records_nothing(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    let mut declared = declarations();
    declared.push(Declaration::new("maps", "Maps"));
    declared.push(Declaration::new("maps", "Other maps"));

    assert_eq!(
        connections.declare_refused(declared).await,
        Err(Refusal::Invalid(Vec::new()))
    );
    assert_eq!(connections.get("maps").await, Err(Refusal::NotFound));
}
