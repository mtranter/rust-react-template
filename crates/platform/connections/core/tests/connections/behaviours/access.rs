use crate::driver::{ADMIN, CRM, Connections, Refusal, USER};
use app_authz::Principal;
use app_db::Columns;
use connections::ConnectionColumns;
use connections_api::events::v1::{ConnectionSecretSet, ConnectionUpdated};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn connections_are_refused_to_a_caller_who_is_not_signed_in(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    connections.sign_out();

    assert_eq!(
        connections.list().await.map(|_| ()),
        Err(Refusal::NotSignedIn)
    );
    assert_eq!(
        connections.get(CRM).await.map(|_| ()),
        Err(Refusal::NotSignedIn)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_who_is_not_an_admin_may_neither_read_nor_change_a_connection(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    let before = connections.get(CRM).await.unwrap();
    connections.sign_in_as_a_user();

    assert_eq!(
        connections.list().await.map(|_| ()),
        Err(Refusal::Forbidden)
    );
    assert_eq!(connections.get(CRM).await, Err(Refusal::Forbidden));
    assert_eq!(
        connections.set_url(CRM, 1, "https://crm.example").await,
        Err(Refusal::Forbidden)
    );
    assert_eq!(
        connections.set_secret(CRM, "api_key", 1, "sk_live").await,
        Err(Refusal::Forbidden)
    );
    assert_eq!(
        connections.clear_secret(CRM, "api_key", 1).await,
        Err(Refusal::Forbidden)
    );

    connections.sign_in_with_roles(&[ADMIN]);
    assert_eq!(connections.get(CRM).await, Ok(before));
    assert!(
        connections
            .announced::<ConnectionUpdated>()
            .await
            .is_empty()
    );
    assert!(
        connections
            .announced::<ConnectionSecretSet>()
            .await
            .is_empty()
    );
}

#[test]
fn every_connection_attribute_has_a_column() {
    ConnectionColumns::assert_every_attribute_is_mapped();
}

/// Keeps the list's SQL and the in-memory check of one Connection from drifting apart, for
/// every action and caller: rerun it after adding a scope or rule.
#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_scoped_query_holds_exactly_the_connections_each_plan_allows(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    let principals = [
        Principal::user("someone", [USER]),
        Principal::user("someone", [ADMIN]),
    ];
    let declared: Vec<_> = crate::driver::declarations()
        .into_iter()
        .map(|d| connections::Connection::declare(d).unwrap().0)
        .collect();

    app_authz::testing::assert_lists_agree(
        &connections.authorizer(),
        &principals,
        &declared,
        |connection| connection.key().to_string(),
        |scope| connections.keys_where::<ConnectionColumns>(scope),
    )
    .await;
}
