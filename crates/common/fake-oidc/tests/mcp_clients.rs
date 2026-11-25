//! Cognito's MCP client support, faked: dynamic registration, client ID
//! metadata documents, and the scope and resource an access token carries.

use app_fake_oidc::{Provider, REGISTER_SCOPE, Subject};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use poem::{
    EndpointExt, Route, Server, get, handler,
    listener::{Acceptor, Listener, TcpListener},
    web::{Data, Json},
};
use reqwest::{StatusCode, redirect::Policy};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use url::Url;

const VERIFIER: &str = "a-pkce-code-verifier-long-enough-to-count-0123456789";
const REDIRECT_URI: &str = "http://127.0.0.1:9/callback";

async fn bind() -> (poem::listener::TcpAcceptor, String) {
    let acceptor = TcpListener::bind("127.0.0.1:0")
        .into_acceptor()
        .await
        .expect("binds");
    let addr = acceptor.local_addr()[0]
        .as_socket_addr()
        .copied()
        .expect("a socket address");
    (acceptor, format!("http://{addr}"))
}

/// The provider on a free port, and itself, for minting tokens.
async fn start(require_token: bool) -> Arc<Provider> {
    let (acceptor, issuer) = bind().await;
    let subject = Subject {
        sub: "ada".into(),
        name: "Ada Lovelace".into(),
        email: "ada@example.com".into(),
    };
    let provider =
        Arc::new(Provider::new(issuer, "spa", subject).require_registration_token(require_token));
    tokio::spawn(Server::new_with_acceptor(acceptor).run(provider.clone().endpoint()));
    provider
}

#[handler]
fn document(Data(doc): Data<&Value>) -> Json<Value> {
    Json(doc.clone())
}

/// Serves a client ID metadata document at `/client.json`, its `client_id` made by
/// `client_id` from the document's own URL.
async fn serve_document(client_id: impl FnOnce(&str) -> String) -> String {
    let (acceptor, base) = bind().await;
    let url = format!("{base}/client.json");
    let doc = json!({
        "client_id": client_id(&url),
        "client_name": "An MCP client",
        "redirect_uris": [REDIRECT_URI],
        "token_endpoint_auth_method": "none",
    });
    tokio::spawn(
        Server::new_with_acceptor(acceptor)
            .run(Route::new().at("/client.json", get(document)).data(doc)),
    );
    url
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(Policy::none())
        .build()
        .expect("a client")
}

async fn authorize(issuer: &str, params: &[(&str, &str)]) -> reqwest::Response {
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(VERIFIER.as_bytes()));
    let mut query = vec![
        ("response_type", "code"),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
    ];
    query.extend_from_slice(params);
    http()
        .get(format!("{issuer}/authorize"))
        .query(&query)
        .send()
        .await
        .expect("answers")
}

/// Signs in as `client_id` and redeems the code, answering the token response.
async fn sign_in(issuer: &str, client_id: &str, extra: &[(&str, &str)]) -> Value {
    let mut params = vec![("client_id", client_id), ("redirect_uri", REDIRECT_URI)];
    params.extend_from_slice(extra);
    let answer = authorize(issuer, &params).await;
    assert_eq!(answer.status(), StatusCode::SEE_OTHER);
    let back = Url::parse(answer.headers()["location"].to_str().expect("text")).expect("a URL");
    let code = back
        .query_pairs()
        .find(|(k, _)| k == "code")
        .expect("a code")
        .1
        .into_owned();
    http()
        .post(format!("{issuer}/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT_URI),
            ("code_verifier", VERIFIER),
            ("client_id", client_id),
        ])
        .send()
        .await
        .expect("answers")
        .error_for_status()
        .expect("issues tokens")
        .json()
        .await
        .expect("JSON")
}

fn claims(jwt: &str) -> Value {
    serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(jwt.split('.').nth(1).expect("claims"))
            .expect("base64url"),
    )
    .expect("JSON")
}

async fn register(issuer: &str, bearer: Option<&str>, body: Value) -> reqwest::Response {
    let mut request = http().post(format!("{issuer}/oauth2/register")).json(&body);
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }
    request.send().await.expect("answers")
}

#[tokio::test]
async fn both_metadata_documents_advertise_registration_and_metadata_documents() {
    let provider = start(false).await;
    let issuer = provider.issuer();
    for path in ["openid-configuration", "oauth-authorization-server"] {
        let meta: Value = http()
            .get(format!("{issuer}/.well-known/{path}"))
            .send()
            .await
            .expect("answers")
            .json()
            .await
            .expect("JSON");
        assert_eq!(
            meta["registration_endpoint"],
            format!("{issuer}/oauth2/register")
        );
        assert_eq!(meta["client_id_metadata_document_supported"], true);
        assert_eq!(
            meta["token_endpoint_auth_methods_supported"],
            json!(["none"])
        );
        assert!(meta["scopes_supported"].as_array().is_some());
    }
}

#[tokio::test]
async fn open_registration_issues_a_client_held_to_its_redirect_uris() {
    let provider = start(false).await;
    let issuer = provider.issuer();
    let answer = register(
        issuer,
        None,
        json!({ "redirect_uris": [REDIRECT_URI], "client_name": "Claude", "scope": "openid" }),
    )
    .await;
    assert_eq!(answer.status(), StatusCode::CREATED);
    let client: Value = answer.json().await.expect("JSON");
    let client_id = client["client_id"].as_str().expect("a client_id");
    assert!(client_id.starts_with("dcr-"), "{client_id}");
    assert_eq!(client["client_name"], "Claude");
    assert_eq!(client["redirect_uris"], json!([REDIRECT_URI]));
    assert!(client["client_id_issued_at"].is_u64());

    let tokens = sign_in(issuer, client_id, &[]).await;
    assert_eq!(
        claims(tokens["access_token"].as_str().expect("a token"))["client_id"],
        client_id
    );

    let elsewhere = authorize(
        issuer,
        &[
            ("client_id", client_id),
            ("redirect_uri", "http://127.0.0.1:9/other"),
        ],
    )
    .await;
    assert_eq!(elsewhere.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn registration_refuses_missing_redirect_uris_and_bad_metadata() {
    let provider = start(false).await;
    let missing = register(provider.issuer(), None, json!({ "client_name": "x" })).await;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
    let error: Value = missing.json().await.expect("JSON");
    assert_eq!(error["error"], "invalid_redirect_uri");

    let secret = register(
        provider.issuer(),
        None,
        json!({ "redirect_uris": [REDIRECT_URI], "token_endpoint_auth_method": "client_secret_basic" }),
    )
    .await;
    let error: Value = secret.json().await.expect("JSON");
    assert_eq!(error["error"], "invalid_client_metadata");
}

#[tokio::test]
async fn protected_registration_needs_a_token_with_the_register_scope() {
    let provider = start(true).await;
    let issuer = provider.issuer();
    let body = json!({ "redirect_uris": [REDIRECT_URI] });

    let anonymous = register(issuer, None, body.clone()).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let unscoped = provider.access_token("spa", "openid", None);
    let refused = register(issuer, Some(&unscoped), body.clone()).await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);

    let scoped = provider.access_token("spa", &format!("openid {REGISTER_SCOPE}"), None);
    let accepted = register(issuer, Some(&scoped), body).await;
    assert_eq!(accepted.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn a_metadata_document_url_signs_in_as_the_client_it_names() {
    let provider = start(false).await;
    let url = serve_document(str::to_owned).await;
    let tokens = sign_in(provider.issuer(), &url, &[]).await;
    let access = claims(tokens["access_token"].as_str().expect("a token"));
    assert_eq!(access["client_id"], url.as_str());
    assert!(access.get("aud").is_none(), "{access}");

    let unlisted = authorize(
        provider.issuer(),
        &[
            ("client_id", &url),
            ("redirect_uri", "http://127.0.0.1:9/other"),
        ],
    )
    .await;
    assert_eq!(unlisted.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_metadata_document_naming_another_client_is_refused() {
    let provider = start(false).await;
    let url = serve_document(|_| "https://elsewhere.example/client.json".into()).await;
    let answer = authorize(
        provider.issuer(),
        &[("client_id", &url), ("redirect_uri", REDIRECT_URI)],
    )
    .await;
    assert_eq!(answer.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_code_is_redeemed_only_by_the_client_it_was_issued_to() {
    let provider = start(false).await;
    let issuer = provider.issuer();
    let answer = authorize(
        issuer,
        &[("client_id", "spa"), ("redirect_uri", REDIRECT_URI)],
    )
    .await;
    let back = Url::parse(answer.headers()["location"].to_str().expect("text")).expect("a URL");
    let code = back
        .query_pairs()
        .find(|(k, _)| k == "code")
        .expect("a code")
        .1
        .into_owned();
    let stolen = http()
        .post(format!("{issuer}/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT_URI),
            ("code_verifier", VERIFIER),
            ("client_id", "dcr-someone-else"),
        ])
        .send()
        .await
        .expect("answers");
    assert_eq!(stolen.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_access_token_carries_the_requested_scope_and_resource() {
    let provider = start(false).await;
    let issuer = provider.issuer();

    let plain = sign_in(issuer, "spa", &[]).await;
    let access = claims(plain["access_token"].as_str().expect("a token"));
    assert_eq!(access["scope"], "openid profile email");
    assert_eq!(access["client_id"], "spa");
    assert_eq!(access["aud"], "spa");

    let scoped = sign_in(
        issuer,
        "spa",
        &[
            ("scope", "openid mcp/tools"),
            ("resource", "https://mcp.example/mcp"),
        ],
    )
    .await;
    assert_eq!(scoped["scope"], "openid mcp/tools");
    let access = claims(scoped["access_token"].as_str().expect("a token"));
    assert_eq!(access["scope"], "openid mcp/tools");
    assert_eq!(access["aud"], "https://mcp.example/mcp");
}
