//! The [`Transport`] adapter: reqwest, on the `ring` rustls provider the tree already has.

use crate::application::{Outbound, Transport};
use connections_api::{CallError, HttpMethod, Response};
use std::time::Duration;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const TIMEOUT: Duration = Duration::from_secs(30);

pub struct HttpTransport {
    client: reqwest::Client,
}

impl HttpTransport {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TIMEOUT)
            // A redirect would carry the Connection's credentials to wherever it points.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("the HTTP client builds");
        Self { client }
    }
}

impl Default for HttpTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Transport for HttpTransport {
    async fn send(&self, request: Outbound) -> Result<Response, CallError> {
        let method = match request.method {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Head => reqwest::Method::HEAD,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Patch => reqwest::Method::PATCH,
            HttpMethod::Delete => reqwest::Method::DELETE,
        };
        let mut builder = self.client.request(method, request.url);
        for (name, value) in request.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        let answer = builder.send().await.map_err(unreachable)?;
        let status = answer.status().as_u16();
        let headers = answer
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned()))
            })
            .collect();
        let body = answer.bytes().await.map_err(unreachable)?.to_vec();
        Ok(Response {
            status,
            headers,
            body,
        })
    }
}

/// Never the URL, whose query may hold an API key, nor reqwest's own message, which names it.
fn unreachable(err: reqwest::Error) -> CallError {
    let why = if err.is_timeout() {
        format!("it didn't answer within {} seconds", TIMEOUT.as_secs())
    } else if err.is_connect() {
        "its host couldn't be reached: check the URL".to_owned()
    } else if err.is_body() || err.is_decode() {
        "its answer was cut off".to_owned()
    } else {
        "the request couldn't be sent".to_owned()
    };
    CallError::Unreachable(why)
}
