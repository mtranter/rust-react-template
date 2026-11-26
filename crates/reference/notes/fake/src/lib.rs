//! A stateful fake of the noticeboard notes share to (ADR 0050): an in-memory server serving
//! the operations notes uses, as `fake-oidc` stands in for an identity provider. Every request
//! must carry `Authorization: Bearer <token>` with the token it was started with, else `401`.
//!
//! - `GET /health`: `200 {"status":"ok"}`, the Connection's health check.
//! - `PUT /pins/:id` with `{"title","body"}`: pins it, replacing a pin of that id (`200`).
//! - `GET /pins`: every pin, by id.
//! - `GET /pins/:id`: one, else `404`.

use poem::http::StatusCode;
use poem::listener::{Acceptor, Listener, TcpListener};
use poem::web::{Data, Json, Path};
use poem::{Endpoint, EndpointExt, IntoResponse, Request, Response, Route, get, handler};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    pub title: String,
    pub body: String,
}

/// The noticeboard's state: its pins by id, and how many times each was put.
pub struct Noticeboard {
    token: String,
    pins: Mutex<BTreeMap<String, Pin>>,
    puts: Mutex<usize>,
}

impl Noticeboard {
    pub fn new(token: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            token: token.into(),
            pins: Mutex::default(),
            puts: Mutex::default(),
        })
    }

    pub fn pins(&self) -> BTreeMap<String, Pin> {
        self.pins.lock().expect("unpoisoned").clone()
    }

    /// Every `PUT` it accepted, repeats included.
    pub fn puts(&self) -> usize {
        *self.puts.lock().expect("unpoisoned")
    }

    pub fn endpoint(self: Arc<Self>) -> impl Endpoint {
        Route::new()
            .at("/health", get(health))
            .at("/pins", get(list))
            .at("/pins/:id", get(read).put(put))
            .data(self)
    }

    /// Served on 127.0.0.1 on a thread of its own, with its own runtime, so it outlives any one
    /// test's: its URL, for the Connection.
    pub fn serve(self: &Arc<Self>) -> String {
        let board = self.clone();
        let (bound, addr) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .expect("a runtime")
                .block_on(async move {
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
                    poem::Server::new_with_acceptor(acceptor)
                        .run(board.endpoint())
                        .await
                })
        });
        format!("http://{}", addr.recv().expect("the noticeboard binds"))
    }

    fn authorised(&self, req: &Request) -> bool {
        req.header("authorization") == Some(&format!("Bearer {}", self.token))
    }
}

type Board<'a> = Data<&'a Arc<Noticeboard>>;

fn unauthorised() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "a bearer token this noticeboard issued is required" })),
    )
        .into_response()
}

#[handler]
fn health(req: &Request, board: Board<'_>) -> Response {
    if !board.authorised(req) {
        return unauthorised();
    }
    Json(json!({ "status": "ok" })).into_response()
}

#[handler]
fn list(req: &Request, board: Board<'_>) -> Response {
    if !board.authorised(req) {
        return unauthorised();
    }
    Json(board.pins()).into_response()
}

#[handler]
fn read(req: &Request, board: Board<'_>, Path(id): Path<String>) -> Response {
    if !board.authorised(req) {
        return unauthorised();
    }
    match board.pins().remove(&id) {
        Some(pin) => Json(pin).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[handler]
fn put(req: &Request, board: Board<'_>, Path(id): Path<String>, Json(pin): Json<Pin>) -> Response {
    if !board.authorised(req) {
        return unauthorised();
    }
    board
        .pins
        .lock()
        .expect("unpoisoned")
        .insert(id, pin.clone());
    *board.puts.lock().expect("unpoisoned") += 1;
    Json(pin).into_response()
}
