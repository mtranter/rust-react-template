//! Binds each MCP session to the token subject that opened it.

use super::discovery::Discovery;
use crate::ApiError;
use crate::auth::{bearer_token, verified_caller};
use crate::render_error;
use app_error::AppError;
use poem::http::Method;
use poem::http::header::WWW_AUTHENTICATE;
use poem::{Endpoint, IntoResponse, Middleware, Request, Response};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::{Duration, Instant};

const SESSION_HEADER: &str = "mcp-session-id";

/// `poem-mcpserver` forgets a session idle this long; so does [`SubjectSessions`].
const SESSION_IDLE: Duration = Duration::from_secs(5 * 60);

/// The token a session's latest request presented, which its tool calls
/// present in turn: so a refreshed token keeps a long session working, and
/// an expired one ends it.
#[derive(Clone)]
pub(super) struct Presented(Arc<RwLock<String>>);

impl Presented {
    fn new(token: String) -> Self {
        Self(Arc::new(RwLock::new(token)))
    }

    pub(super) fn token(&self) -> String {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn present(&self, token: String) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = token;
    }
}

/// Authenticates every request, not only the one opening a session, which is
/// all `poem-mcpserver` sees a token on: later ones it routes by
/// `Mcp-Session-Id` alone. So a leaked session id is useless without a token
/// of the subject who opened it.
pub(super) struct SubjectSessions {
    sessions: Arc<Mutex<HashMap<String, Owner>>>,
    discovery: Discovery,
}

impl SubjectSessions {
    pub(super) fn new(discovery: Discovery) -> Self {
        Self {
            sessions: Arc::default(),
            discovery,
        }
    }
}

struct Owner {
    subject: String,
    presented: Presented,
    seen: Instant,
}

impl<E: Endpoint> Middleware<E> for SubjectSessions {
    type Output = SubjectSessionsEndpoint<E>;

    fn transform(&self, inner: E) -> Self::Output {
        SubjectSessionsEndpoint {
            inner,
            sessions: self.sessions.clone(),
            discovery: self.discovery.clone(),
        }
    }
}

pub(super) struct SubjectSessionsEndpoint<E> {
    inner: E,
    sessions: Arc<Mutex<HashMap<String, Owner>>>,
    discovery: Discovery,
}

impl<E> SubjectSessionsEndpoint<E> {
    fn sessions(&self) -> MutexGuard<'_, HashMap<String, Owner>> {
        self.sessions.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The session `id` if `subject` opened it, now presenting `token`.
    /// Anyone else's session is as unknown as one never opened, so its
    /// existence isn't revealed.
    fn resume(&self, id: &str, subject: &str, token: &str) -> Option<Presented> {
        match self.sessions().get_mut(id) {
            Some(owner) if owner.subject == subject => {
                owner.seen = Instant::now();
                owner.presented.present(token.to_owned());
                Some(owner.presented.clone())
            }
            _ => None,
        }
    }

    fn open(&self, id: String, subject: String, presented: Presented) {
        let mut sessions = self.sessions();
        sessions.retain(|_, owner| owner.seen.elapsed() < SESSION_IDLE);
        sessions.insert(
            id,
            Owner {
                subject,
                presented,
                seen: Instant::now(),
            },
        );
    }

    fn close(&self, id: &str) {
        self.sessions().remove(id);
    }
}

impl<E: Endpoint> Endpoint for SubjectSessionsEndpoint<E> {
    type Output = Response;

    async fn call(&self, mut req: Request) -> poem::Result<Response> {
        let (Some(claims), Some(token)) = (
            verified_caller(&req).await,
            bearer_token(&req).map(str::to_owned),
        ) else {
            let refused = refusal(AppError::Unauthorized);
            return Ok(match self.discovery.challenge(&req) {
                Some(challenge) => refused
                    .with_header(WWW_AUTHENTICATE, challenge)
                    .into_response(),
                None => refused,
            });
        };
        let session = req
            .headers()
            .get(SESSION_HEADER)
            .and_then(|id| id.to_str().ok())
            .map(str::to_owned);
        let presented = match &session {
            Some(id) => match self.resume(id, &claims.sub, &token) {
                Some(presented) => presented,
                None => return Ok(no_session()),
            },
            None => Presented::new(token),
        };
        let closing = req.method() == Method::DELETE;
        req.set_data(presented.clone());
        let response = self.inner.call(req).await?.into_response();
        let status = response.status();
        match session {
            None => {
                let opened = response
                    .headers()
                    .get(SESSION_HEADER)
                    .and_then(|id| id.to_str().ok());
                if let Some(id) = opened {
                    self.open(id.to_owned(), claims.sub, presented);
                }
            }
            Some(id) if status.as_u16() == 404 || (closing && status.is_success()) => {
                self.close(&id);
            }
            Some(_) => {}
        }
        // `poem-mcpserver` refuses with a bare status; every refusal has the
        // one error shape.
        Ok(match status.as_u16() {
            404 => no_session(),
            400..500 => render_error(poem::Error::from_status(status)).await,
            _ => response,
        })
    }
}

fn no_session() -> Response {
    refusal(AppError::not_found("session"))
}

fn refusal(error: AppError) -> Response {
    ApiError::from(error).into_response()
}
