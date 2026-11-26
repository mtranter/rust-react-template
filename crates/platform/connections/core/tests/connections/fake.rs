//! A stateful fake of another system, as an integration's tests would write one (ADR 0050):
//! served on 127.0.0.1, it answers `/health` 200 only to the credentials it was told, and
//! issues OAuth 2 client-credentials tokens at `/oauth/token`, recording every request.

use poem::http::StatusCode;
use poem::web::{Data, Form, Query};
use poem::{EndpointExt, IntoResponse, Request, Response, Route, handler, post};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const TOKEN: &str = "tok_live_0123456789abcdef";
pub const USERNAME: &str = "bot";
pub const PASSWORD: &str = "pw_live_0123456789abcdef";
pub const API_KEY: &str = "sk_live_0123456789abcdef";
pub const CLIENT_ID: &str = "client-0080";
pub const CLIENT_SECRET: &str = "cs_live_0123456789abcdef";
/// What the fake issues for the client credentials: never set by an admin.
pub const ISSUED: &str = "at_issued_0123456789abcdef";
/// Every secret above: none may show in a span or a log.
pub const SECRETS: &[&str] = &[TOKEN, PASSWORD, API_KEY, CLIENT_SECRET, ISSUED];

/// A request as the fake saw it.
#[derive(Debug, Clone)]
pub struct Seen {
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
}

#[derive(Default)]
struct State {
    seen: Vec<Seen>,
    tokens_issued: usize,
}

#[derive(Clone, Default)]
pub struct FakeSystem {
    state: Arc<Mutex<State>>,
    pub url: String,
}

impl FakeSystem {
    pub fn serve() -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let shared = state.clone();
        let (bound, addr) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .expect("a runtime")
                .block_on(async move {
                    use poem::listener::{Acceptor, Listener, TcpListener};
                    let acceptor = TcpListener::bind("127.0.0.1:0")
                        .into_acceptor()
                        .await
                        .expect("binds");
                    let addr = acceptor
                        .local_addr()
                        .into_iter()
                        .find_map(|addr| addr.as_socket_addr().copied())
                        .expect("a socket address");
                    bound.send(addr).expect("the caller waits for it");
                    let app = Route::new()
                        .at("/oauth/token", post(token))
                        .at("/*path", health)
                        .data(shared);
                    poem::Server::new_with_acceptor(acceptor).run(app).await
                })
        });
        let url = format!("http://{}", addr.recv().expect("the fake binds"));
        Self { state, url }
    }

    pub fn last(&self) -> Seen {
        self.state
            .lock()
            .unwrap()
            .seen
            .last()
            .cloned()
            .expect("a request")
    }

    pub fn tokens_issued(&self) -> usize {
        self.state.lock().unwrap().tokens_issued
    }
}

fn seen(req: &Request) -> Seen {
    Seen {
        path: req.uri().path().to_owned(),
        query: url::form_urlencoded::parse(req.uri().query().unwrap_or_default().as_bytes())
            .into_owned()
            .collect(),
        headers: req
            .headers()
            .iter()
            .map(|(n, v)| (n.as_str().to_owned(), v.to_str().unwrap_or("").to_owned()))
            .collect(),
    }
}

fn basic(user: &str, password: &str) -> String {
    use base64::Engine;
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
    )
}

#[handler]
fn health(req: &Request, state: Data<&Arc<Mutex<State>>>) -> Response {
    let seen = seen(req);
    let authorization = seen.headers.get("authorization").cloned();
    let ok = authorization.as_deref() == Some(&format!("Bearer {TOKEN}"))
        || authorization.as_deref() == Some(&format!("Bearer {ISSUED}"))
        || authorization.as_deref() == Some(&basic(USERNAME, PASSWORD))
        || seen.headers.get("x-api-key").map(String::as_str) == Some(API_KEY)
        || seen.query.get("key").map(String::as_str) == Some(API_KEY)
        || seen.path.ends_with("/public");
    state.0.lock().unwrap().seen.push(seen);
    if ok {
        Json(json!({ "status": "ok" })).into_response()
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

use poem::web::Json;

#[derive(Deserialize)]
struct Grant {
    grant_type: String,
    scope: Option<String>,
}

#[handler]
fn token(
    req: &Request,
    state: Data<&Arc<Mutex<State>>>,
    Form(grant): Form<Grant>,
    _: Query<HashMap<String, String>>,
) -> Response {
    let seen = seen(req);
    let authorised = seen.headers.get("authorization").map(String::as_str)
        == Some(&basic(CLIENT_ID, CLIENT_SECRET));
    let mut state = state.0.lock().unwrap();
    state.seen.push(seen);
    if grant.grant_type != "client_credentials" || grant.scope.as_deref() != Some("orders.read") {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if !authorised {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid_client" })),
        )
            .into_response();
    }
    state.tokens_issued += 1;
    Json(json!({ "access_token": ISSUED, "token_type": "Bearer", "expires_in": 3600 }))
        .into_response()
}
