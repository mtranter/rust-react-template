use crate::driver::{BILLING, CRM, Connections, ERP, FEED, LEDGER, Refusal, SEARCH};
use crate::fake::{
    API_KEY, CLIENT_ID, CLIENT_SECRET, FakeSystem, ISSUED, PASSWORD, SECRETS, TOKEN, USERNAME,
};
use app_web::testing::Traces;
use base64::Engine;
use connections_api::{CallError, Request};
use serde_json::json;
use sqlx::PgPool;

async fn health(connections: &Connections, key: &str) -> u16 {
    let client = connections.clients().client(key).await.expect("configured");
    client
        .send(Request::get("/health"))
        .await
        .expect("answered")
        .status
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn each_auth_style_sends_its_credentials_as_the_other_system_expects(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    let fake = FakeSystem::serve();
    let api = format!("{}/api", fake.url);

    connections
        .configure(BILLING, &api, json!({}), &[("token", TOKEN)])
        .await;
    assert_eq!(health(&connections, BILLING).await, 200);
    let seen = fake.last();
    assert_eq!(seen.path, "/api/health");
    assert_eq!(seen.headers["authorization"], format!("Bearer {TOKEN}"));

    connections
        .configure(
            LEDGER,
            &api,
            json!({ "username": USERNAME }),
            &[("password", PASSWORD)],
        )
        .await;
    assert_eq!(health(&connections, LEDGER).await, 200);
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("{USERNAME}:{PASSWORD}"));
    assert_eq!(
        fake.last().headers["authorization"],
        format!("Basic {basic}")
    );

    connections
        .configure(
            CRM,
            &api,
            json!({ "tenant": "acme" }),
            &[("api_key", API_KEY)],
        )
        .await;
    assert_eq!(health(&connections, CRM).await, 200);
    let seen = fake.last();
    assert_eq!(seen.headers["x-api-key"], API_KEY);
    assert!(!seen.headers.contains_key("authorization"));

    connections
        .configure(SEARCH, &api, json!({}), &[("api_key", API_KEY)])
        .await;
    let client = connections.clients().client(SEARCH).await.unwrap();
    let answer = client
        .send(Request::get("/health").query("q", "ada"))
        .await
        .unwrap();
    assert_eq!(answer.status, 200);
    let seen = fake.last();
    assert_eq!(seen.query["key"], API_KEY);
    assert_eq!(seen.query["q"], "ada");

    connections.configure(FEED, &api, json!({}), &[]).await;
    let client = connections.clients().client(FEED).await.unwrap();
    assert_eq!(
        client.send(Request::get("/public")).await.unwrap().status,
        200
    );
    let seen = fake.last();
    assert!(!seen.headers.contains_key("authorization"));
    assert!(seen.query.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_oauth_client_credentials_token_is_asked_for_once_and_sent_as_a_bearer_until_it_expires(
    pool: PgPool,
) {
    let connections = Connections::seeded(pool).await;
    let fake = FakeSystem::serve();
    connections
        .configure(
            ERP,
            &fake.url,
            json!({ "token_url": format!("{}/oauth/token", fake.url), "client_id": CLIENT_ID }),
            &[("client_secret", CLIENT_SECRET)],
        )
        .await;

    assert_eq!(health(&connections, ERP).await, 200);
    assert_eq!(
        fake.last().headers["authorization"],
        format!("Bearer {ISSUED}")
    );
    assert_eq!(health(&connections, ERP).await, 200);
    assert_eq!(fake.tokens_issued(), 1);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_connection_not_filled_in_is_not_configured_naming_what_it_lacks(pool: PgPool) {
    let connections = Connections::seeded(pool).await;

    let Err(refused) = connections.clients().client(CRM).await else {
        panic!("a Connection with no URL is no client");
    };
    assert_eq!(refused.code(), "connection_not_configured");
    assert_eq!(
        refused,
        CallError::NotConfigured("the Connection `crm` lacks its URL, API key".into())
    );
    assert!(matches!(
        connections.clients().client("nowhere").await,
        Err(CallError::Undeclared(_))
    ));
    assert_eq!(connections.test(CRM).await, Err(Refusal::NotConfigured));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn testing_a_connection_reports_ok_then_failing_with_a_readable_message_after_a_bad_secret(
    pool: PgPool,
) {
    let connections = Connections::seeded(pool).await;
    let fake = FakeSystem::serve();
    let configured = connections
        .configure(CRM, &fake.url, json!({}), &[("api_key", API_KEY)])
        .await;
    assert_eq!(configured["status"], "untested");
    assert_eq!(
        connections.get(BILLING).await.unwrap()["status"],
        "not_configured"
    );

    let tested = connections.test(CRM).await.unwrap();
    assert_eq!(tested["status"], "ok");
    assert_eq!(tested["lastError"], json!(null));
    assert_eq!(tested["version"], configured["version"]);
    let listed = connections.list().await.unwrap();
    let crm = listed.items.iter().find(|c| c["key"] == CRM).unwrap();
    assert_eq!(crm["status"], "ok");

    let version = tested["version"].as_str().unwrap().parse().unwrap();
    let changed = connections
        .set_secret(CRM, "api_key", version, "sk_wrong")
        .await
        .unwrap();
    assert_eq!(changed["status"], "untested");

    let failing = connections.test(CRM).await.unwrap();
    assert_eq!(failing["status"], "failing");
    assert_eq!(
        failing["lastError"],
        "GET /health answered 401 Unauthorized: check the credentials"
    );
    let listed = connections.list().await.unwrap();
    let crm = listed.items.iter().find(|c| c["key"] == CRM).unwrap();
    assert_eq!(crm["status"], "failing");
    assert_eq!(crm["lastError"], failing["lastError"]);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_connection_that_cant_be_reached_fails_saying_so(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    // A port nothing listens on: bound, then dropped.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    connections
        .configure(
            CRM,
            &format!("http://127.0.0.1:{port}"),
            json!({}),
            &[("api_key", API_KEY)],
        )
        .await;

    let failing = connections.test(CRM).await.unwrap();
    assert_eq!(failing["status"], "failing");
    assert_eq!(
        failing["lastError"],
        "GET /health failed: its host couldn't be reached: check the URL"
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn no_secret_appears_in_the_spans_of_a_call(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    let fake = FakeSystem::serve();
    connections
        .configure(SEARCH, &fake.url, json!({}), &[("api_key", API_KEY)])
        .await;
    connections
        .configure(BILLING, &fake.url, json!({}), &[("token", TOKEN)])
        .await;
    connections
        .configure(
            ERP,
            &fake.url,
            json!({ "token_url": format!("{}/oauth/token", fake.url), "client_id": CLIENT_ID }),
            &[("client_secret", CLIENT_SECRET)],
        )
        .await;
    connections
        .configure(
            CRM,
            &fake.url,
            json!({}),
            &[("api_key", "sk_wrong_0123456789")],
        )
        .await;

    let traces = Traces::install();
    for key in [SEARCH, BILLING, ERP] {
        assert_eq!(health(&connections, key).await, 200);
    }
    connections.test(CRM).await.unwrap();

    let spans = traces.spans();
    let call = spans
        .iter()
        .find(|s| s.name == "GET search")
        .expect("a call is one client span, named by its method and Connection");
    assert_eq!(
        Traces::attribute(call, "url.path").as_deref(),
        Some("/health")
    );
    assert_eq!(
        Traces::attribute(call, "http.response.status_code").as_deref(),
        Some("200")
    );
    assert!(spans.iter().any(|s| s.name == "connection_token"));
    let exported = format!("{spans:?}");
    for secret in SECRETS.iter().chain(&["sk_wrong_0123456789"]) {
        assert!(!exported.contains(secret), "a span holds {secret}");
    }
}

/// Every log line written while calling, at every level, reqwest's and hyper's included.
#[derive(Clone, Default)]
struct Captured(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn no_secret_appears_in_the_logs_of_a_call(pool: PgPool) {
    let connections = Connections::seeded(pool).await;
    let fake = FakeSystem::serve();
    connections
        .configure(SEARCH, &fake.url, json!({}), &[("api_key", API_KEY)])
        .await;
    connections
        .configure(
            ERP,
            &fake.url,
            json!({ "token_url": format!("{}/oauth/token", fake.url), "client_id": CLIENT_ID }),
            &[("client_secret", CLIENT_SECRET)],
        )
        .await;
    connections
        .configure(CRM, &fake.url, json!({}), &[("api_key", API_KEY)])
        .await;

    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let logged = {
        let _installed = tracing::subscriber::set_default(subscriber);
        for key in [SEARCH, ERP] {
            assert_eq!(health(&connections, key).await, 200);
        }
        connections.test(CRM).await.unwrap();
        String::from_utf8(captured.0.lock().unwrap().clone()).unwrap()
    };

    assert!(
        logged.contains("connection_call"),
        "nothing was logged: {logged}"
    );
    for secret in SECRETS {
        assert!(!logged.contains(secret), "a log line holds {secret}");
    }
}
