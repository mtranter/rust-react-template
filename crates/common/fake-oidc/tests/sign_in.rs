use app_fake_oidc::{Provider, Subject};
use app_web::JwtVerifier;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use poem::{
    Server,
    listener::{Acceptor, Listener, TcpListener},
};
use reqwest::{StatusCode, redirect::Policy};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use url::Url;

const CLIENT_ID: &str = "spa";
const REDIRECT_URI: &str = "http://app.test/auth/callback";
const VERIFIER: &str = "a-pkce-code-verifier-long-enough-to-count-0123456789";

fn ada() -> Subject {
    Subject {
        sub: "ada".to_string(),
        name: "Ada Lovelace".to_string(),
        email: "ada@example.com".to_string(),
    }
}

/// The provider on a free port, and its issuer.
async fn start() -> String {
    let acceptor = TcpListener::bind("127.0.0.1:0")
        .into_acceptor()
        .await
        .expect("binds");
    let addr = acceptor.local_addr()[0]
        .as_socket_addr()
        .copied()
        .expect("a socket address");
    let provider = Arc::new(Provider::new(format!("http://{addr}"), CLIENT_ID, ada()));
    let issuer = provider.issuer().to_string();
    tokio::spawn(Server::new_with_acceptor(acceptor).run(provider.endpoint()));
    issuer
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(Policy::none())
        .build()
        .expect("a client")
}

/// Approves at the authorize endpoint, answering the code it redirects back with.
async fn authorize(issuer: &str, challenge: &str) -> String {
    let answer = http()
        .get(format!("{issuer}/authorize"))
        .query(&[
            ("client_id", CLIENT_ID),
            ("redirect_uri", REDIRECT_URI),
            ("response_type", "code"),
            ("scope", "openid profile email"),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
            ("state", "the-state"),
            ("nonce", "the-nonce"),
        ])
        .send()
        .await
        .expect("answers");
    assert_eq!(answer.status(), StatusCode::SEE_OTHER);
    let back = Url::parse(answer.headers()["location"].to_str().expect("text")).expect("a URL");
    assert!(back.as_str().starts_with(REDIRECT_URI), "{back}");
    let param = |name: &str| {
        back.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    assert_eq!(param("state").as_deref(), Some("the-state"));
    param("code").expect("a code")
}

async fn redeem(issuer: &str, code: &str, verifier: &str) -> reqwest::Response {
    http()
        .post(format!("{issuer}/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
            ("code_verifier", verifier),
            ("client_id", CLIENT_ID),
        ])
        .send()
        .await
        .expect("answers")
}

fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[tokio::test]
async fn its_access_token_is_verified_through_its_discovered_jwks_as_the_named_subject() {
    let issuer = start().await;
    let code = authorize(&issuer, &challenge(VERIFIER)).await;

    let tokens: Value = redeem(&issuer, &code, VERIFIER)
        .await
        .error_for_status()
        .expect("issues tokens")
        .json()
        .await
        .expect("JSON");

    let verifier = JwtVerifier::from_discovery(&issuer, None)
        .await
        .expect("discovers");
    let access = tokens["access_token"].as_str().expect("an access token");
    let claims = verifier.verify(access).await.expect("verifies");
    assert_eq!(claims.sub, "ada");
    assert_eq!(claims.name.as_deref(), Some("Ada Lovelace"));
    assert_eq!(claims.email.as_deref(), Some("ada@example.com"));
    let header = jsonwebtoken::decode_header(access).expect("a JWT");
    assert_eq!(header.alg, jsonwebtoken::Algorithm::RS256);
    let id_token = tokens["id_token"].as_str().expect("an ID token");
    let id_claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(id_token.split('.').nth(1).expect("claims"))
            .expect("base64url"),
    )
    .expect("JSON");
    assert_eq!(id_claims["nonce"], "the-nonce");
    assert_eq!(id_claims["aud"], CLIENT_ID);
}

#[tokio::test]
async fn a_code_is_redeemed_only_with_its_pkce_verifier_and_only_once() {
    let issuer = start().await;
    let code = authorize(&issuer, &challenge(VERIFIER)).await;

    let wrong = redeem(&issuer, &code, "another-verifier").await;
    assert_eq!(wrong.status(), StatusCode::BAD_REQUEST);
    let code = authorize(&issuer, &challenge(VERIFIER)).await;
    assert!(redeem(&issuer, &code, VERIFIER).await.status().is_success());
    let again = redeem(&issuer, &code, VERIFIER).await;
    assert_eq!(again.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn signing_out_returns_to_where_the_client_asked() {
    let issuer = start().await;
    let answer = http()
        .get(format!("{issuer}/end-session"))
        .query(&[(
            "post_logout_redirect_uri",
            "http://app.test/auth/signed-out",
        )])
        .send()
        .await
        .expect("answers");
    assert_eq!(answer.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        answer.headers()["location"],
        "http://app.test/auth/signed-out"
    );
}
