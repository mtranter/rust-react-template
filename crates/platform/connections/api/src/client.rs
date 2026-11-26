use std::fmt;
use std::sync::Arc;

/// What a module calls another system through (ADR 0050): `client("crm")` answers a client with
/// the Connection's URL, auth, timeouts and tracing applied, and its secrets kept out of spans
/// and logs. Call out from a job or from the request that needs the answer, never inside an
/// outbox handler's unit of work.
#[async_trait::async_trait]
pub trait Clients: Send + Sync {
    /// Refused [`CallError::NotConfigured`] until an admin has given it a URL and every secret
    /// and field its auth style takes.
    async fn client(&self, key: &str) -> Result<Arc<dyn Client>, CallError>;
}

/// A Connection's client: each request's path is relative to the Connection's URL.
#[async_trait::async_trait]
pub trait Client: Send + Sync {
    /// Any status the other system answers is `Ok`: only a request never answered is an error.
    async fn send(&self, request: Request) -> Result<Response, CallError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Head => "HEAD",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
        }
    }
}

impl From<crate::Method> for HttpMethod {
    fn from(method: crate::Method) -> Self {
        match method {
            crate::Method::Get => HttpMethod::Get,
            crate::Method::Head => HttpMethod::Head,
            crate::Method::Post => HttpMethod::Post,
        }
    }
}

/// ```
/// use connections_api::Request;
///
/// let request = Request::post("/contacts")
///     .query("dry_run", "true")
///     .json(&serde_json::json!({ "name": "Ada" }));
/// assert_eq!(request.path, "/contacts");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: HttpMethod,
    /// Relative to the Connection's URL, from `/`.
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

impl Request {
    pub fn new(method: HttpMethod, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn get(path: impl Into<String>) -> Self {
        Self::new(HttpMethod::Get, path)
    }

    pub fn post(path: impl Into<String>) -> Self {
        Self::new(HttpMethod::Post, path)
    }

    pub fn put(path: impl Into<String>) -> Self {
        Self::new(HttpMethod::Put, path)
    }

    pub fn patch(path: impl Into<String>) -> Self {
        Self::new(HttpMethod::Patch, path)
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self::new(HttpMethod::Delete, path)
    }

    pub fn query(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.push((name.into(), value.into()));
        self
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Sets `Content-Type: application/json`.
    pub fn json<T: serde::Serialize>(self, body: &T) -> Self {
        let body = serde_json::to_vec(body).expect("a serializable body");
        Self {
            body: Some(body),
            ..self.header("Content-Type", "application/json")
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, CallError> {
        serde_json::from_slice(&self.body).map_err(|err| {
            CallError::Unreadable(format!("the answer isn't the JSON expected: {err}"))
        })
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("status", &self.status)
            .field("body_len", &self.body.len())
            .finish_non_exhaustive()
    }
}

/// Every message is safe to log and show: none holds a secret.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallError {
    /// `connection_not_configured`: a domain error, not a 500. An admin fills the Connection in.
    #[error("connection_not_configured: {0}")]
    NotConfigured(String),
    /// No module declares a Connection by that key: a mistake in code.
    #[error("no Connection `{0}` is declared")]
    Undeclared(String),
    /// Refused an OAuth 2 token by the token endpoint.
    #[error("{0}")]
    Unauthorized(String),
    /// Sent, but never answered: unreachable, refused, or too slow.
    #[error("{0}")]
    Unreachable(String),
    /// Answered, but not as expected.
    #[error("{0}")]
    Unreadable(String),
}

impl CallError {
    pub fn code(&self) -> &'static str {
        match self {
            CallError::NotConfigured(_) => "connection_not_configured",
            CallError::Undeclared(_) => "connection_undeclared",
            CallError::Unauthorized(_) => "connection_unauthorized",
            CallError::Unreachable(_) => "connection_unreachable",
            CallError::Unreadable(_) => "connection_unreadable",
        }
    }
}

impl From<CallError> for app_error::AppError {
    fn from(err: CallError) -> Self {
        match err {
            CallError::NotConfigured(_) => app_error::AppError::not_configured(err.to_string()),
            CallError::Undeclared(_) => app_error::AppError::internal(err),
            CallError::Unauthorized(_) | CallError::Unreachable(_) | CallError::Unreadable(_) => {
                app_error::AppError::unavailable(err.to_string())
            }
        }
    }
}
