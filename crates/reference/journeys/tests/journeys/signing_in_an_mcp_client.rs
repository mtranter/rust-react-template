//! An MCP client finding out how to sign in and doing so, over plain HTTP as
//! a real one would (ADR 0049): the server's challenge, its Protected Resource
//! Metadata, the issuer's metadata, then a client ID metadata document or a
//! dynamic registration, and the authorization code with PKCE.

use crate::driver::{App, MCP_SCOPE, Served, bind};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use poem::{
    EndpointExt, Route, get, handler,
    web::{Data, Json},
};
use reqwest::{Client, StatusCode, redirect::Policy};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use url::Url;
use users_api::seed::USER_ID_ALICE;

const VERIFIER: &str = "a-pkce-code-verifier-long-enough-to-count-0123456789";
const REDIRECT_URI: &str = "http://127.0.0.1:9/callback";

fn client() -> Client {
    Client::builder()
        .redirect(Policy::none())
        .build()
        .expect("an HTTP client")
}

/// A `WWW-Authenticate` parameter's quoted value.
fn param(challenge: &str, name: &str) -> Option<String> {
    let start = challenge.find(&format!("{name}=\""))? + name.len() + 2;
    let len = challenge[start..].find('"')?;
    Some(challenge[start..start + len].to_owned())
}

/// What the client learned before signing in.
struct Learned {
    mcp: String,
    scope: String,
    authorization_server: Url,
    issuer: Value,
}

/// Steps 1 to 3: challenged, then reading the two metadata documents.
async fn discover(http: &Client, served: &Served) -> Learned {
    let mcp = format!("{}/mcp", served.url);
    let refused = http
        .post(&mcp)
        .header("accept", "application/json, text/event-stream")
        .json(&initialize())
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    let challenge = refused.headers()["www-authenticate"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(challenge.starts_with("Bearer "), "{challenge}");
    let metadata_url = param(&challenge, "resource_metadata").expect("resource_metadata");
    let scope = param(&challenge, "scope").expect("scope");
    assert_eq!(scope, MCP_SCOPE);

    let prm: Value = http
        .get(&metadata_url)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(prm["resource"], mcp);
    assert_eq!(prm["scopes_supported"], json!([MCP_SCOPE]));
    let authorization_server =
        Url::parse(prm["authorization_servers"][0].as_str().unwrap()).unwrap();
    assert_eq!(
        authorization_server.as_str().trim_end_matches('/'),
        served.provider.issuer()
    );

    let issuer: Value = http
        .get(format!(
            "{}/.well-known/oauth-authorization-server",
            authorization_server.as_str().trim_end_matches('/')
        ))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(issuer["code_challenge_methods_supported"], json!(["S256"]));
    Learned {
        mcp,
        scope,
        authorization_server,
        issuer,
    }
}

/// Step 4: the authorization code with PKCE, asking for `scope` for the MCP
/// resource as `client_id`; the access token.
async fn sign_in(http: &Client, learned: &Learned, client_id: &str, scope: &str) -> String {
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(VERIFIER.as_bytes()));
    let mut authorize =
        Url::parse(learned.issuer["authorization_endpoint"].as_str().unwrap()).unwrap();
    authorize
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", "xyz")
        .append_pair("scope", scope)
        .append_pair("resource", &learned.mcp);
    let approved = http.get(authorize).send().await.unwrap();
    assert!(approved.status().is_redirection(), "{}", approved.status());
    let back = Url::parse(approved.headers()["location"].to_str().unwrap()).unwrap();
    assert!(back.as_str().starts_with(REDIRECT_URI));
    let query = |name: &str| {
        back.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    assert_eq!(query("state").as_deref(), Some("xyz"));
    let code = query("code").expect("a code");

    let tokens: Value = http
        .post(learned.issuer["token_endpoint"].as_str().unwrap())
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT_URI),
            ("code_verifier", VERIFIER),
            ("client_id", client_id),
        ])
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    tokens["access_token"].as_str().unwrap().to_owned()
}

fn initialize() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "journey-client", "version": "1.0" },
        },
    })
}

/// Opens a session with `token` and lists the tools: their names.
async fn list_tools(http: &Client, mcp: &str, token: &str) -> Vec<String> {
    let opened = http
        .post(mcp)
        .bearer_auth(token)
        .header("accept", "application/json")
        .json(&initialize())
        .send()
        .await
        .unwrap();
    assert_eq!(opened.status(), StatusCode::OK);
    let session = opened.headers()["mcp-session-id"]
        .to_str()
        .unwrap()
        .to_owned();
    let call = |body: Value| {
        http.post(mcp)
            .bearer_auth(token)
            .header("accept", "application/json")
            .header("mcp-session-id", &session)
            .json(&body)
            .send()
    };
    let initialized = call(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .await
        .unwrap();
    assert!(initialized.status().is_success());
    let listed = call(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} }))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    // Answered as a batch, as the server answers every message.
    let listed: Value = listed.json().await.unwrap();
    let listed = listed
        .as_array()
        .and_then(|batch| batch.last())
        .unwrap_or(&listed);
    listed["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect()
}

#[handler]
fn document(Data(doc): Data<&Value>) -> Json<Value> {
    Json(doc.clone())
}

/// A client ID metadata document served at its own URL: the client ID.
async fn serve_client_document() -> String {
    let (acceptor, base) = bind().await;
    let client_id = format!("{base}/client.json");
    let doc = json!({
        "client_id": client_id,
        "client_name": "A journey's MCP client",
        "redirect_uris": [REDIRECT_URI],
        "token_endpoint_auth_method": "none",
        "grant_types": ["authorization_code"],
        "response_types": ["code"],
    });
    tokio::spawn(
        poem::Server::new_with_acceptor(acceptor)
            .run(Route::new().at("/client.json", get(document)).data(doc)),
    );
    client_id
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_client_with_a_metadata_document_discovers_signs_in_and_lists_tools(pool: PgPool) {
    let app = App::seeded(pool).await;
    let served = app.served_behind_an_issuer(USER_ID_ALICE).await;
    let http = client();
    let learned = discover(&http, &served).await;
    assert_eq!(
        learned.issuer["client_id_metadata_document_supported"],
        true
    );
    assert!(
        learned
            .authorization_server
            .as_str()
            .starts_with("http://127.0.0.1")
    );

    let client_id = serve_client_document().await;
    let token = sign_in(
        &http,
        &learned,
        &client_id,
        &format!("openid {}", learned.scope),
    )
    .await;

    let tools = list_tools(&http, &learned.mcp, &token).await;
    assert!(tools.iter().any(|tool| tool == "get_me"), "{tools:?}");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_dynamically_registered_client_discovers_signs_in_and_lists_tools(pool: PgPool) {
    let app = App::seeded(pool).await;
    let served = app.served_behind_an_issuer(USER_ID_ALICE).await;
    let http = client();
    let learned = discover(&http, &served).await;

    let registered = http
        .post(learned.issuer["registration_endpoint"].as_str().unwrap())
        .json(&json!({
            "client_name": "A journey's MCP client",
            "redirect_uris": [REDIRECT_URI],
            "token_endpoint_auth_method": "none",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(registered.status(), StatusCode::CREATED);
    let registered: Value = registered.json().await.unwrap();
    let client_id = registered["client_id"].as_str().unwrap();
    let token = sign_in(&http, &learned, client_id, &learned.scope).await;

    let tools = list_tools(&http, &learned.mcp, &token).await;
    assert!(tools.iter().any(|tool| tool == "get_me"), "{tools:?}");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_token_not_meant_for_mcp_is_refused_for_want_of_its_scope(pool: PgPool) {
    let app = App::seeded(pool).await;
    let served = app.served_behind_an_issuer(USER_ID_ALICE).await;
    let http = client();
    let learned = discover(&http, &served).await;

    // The SPA's own sign-in: the default scopes, for no resource.
    let token = served
        .provider
        .access_token("spa", app_fake_oidc::DEFAULT_SCOPE, None);
    let refused = http
        .post(&learned.mcp)
        .bearer_auth(&token)
        .header("accept", "application/json")
        .json(&initialize())
        .send()
        .await
        .unwrap();

    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    let challenge = refused.headers()["www-authenticate"].to_str().unwrap();
    assert_eq!(
        param(challenge, "error").as_deref(),
        Some("insufficient_scope")
    );
    assert_eq!(param(challenge, "scope").as_deref(), Some(MCP_SCOPE));
}
