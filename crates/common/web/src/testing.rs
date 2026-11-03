//! The transport half of a module's test driver (spec §13), behind the `test`
//! feature, which only dev-dependencies enable. The driver keeps what is its
//! module's own: domain verbs, JSON newtypes built from aggregates, and a
//! `Refusal` declaring only the codes the module produces.

use crate::{RequestIdMiddleware, render_error};
use poem::endpoint::BoxEndpoint;
use poem::test::{TestClient, TestRequestBuilder};
use poem::{Endpoint, EndpointExt, Route};
use poem_openapi::{OpenApi, OpenApiService};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::fmt::Display;
use std::ops::Deref;

pub struct TestApi {
    client: TestClient<BoxEndpoint<'static>>,
}

impl TestApi {
    /// Mounts `endpoints` under `/api` with the server's error rendering and
    /// request ids; `wire` adds the data they read (a provider, a verifier).
    pub fn new<T, E>(endpoints: T, wire: impl FnOnce(Route) -> E) -> Self
    where
        T: OpenApi + 'static,
        E: Endpoint + 'static,
    {
        let api = OpenApiService::new(endpoints, "test", "1.0");
        let app = wire(Route::new().nest("/api", api))
            .catch_all_error(render_error)
            .with(RequestIdMiddleware)
            .boxed();
        Self {
            client: TestClient::new(app),
        }
    }

    /// Over a whole app that already renders errors and stamps request ids, as
    /// the composed server's `build_app` does.
    pub fn over<E>(app: E) -> Self
    where
        E: Endpoint + 'static,
    {
        Self {
            client: TestClient::new(app.map_to_response().boxed()),
        }
    }

    pub fn get(&self, uri: impl Into<String>) -> Call<'_> {
        Call(self.client.get(uri))
    }

    pub fn post(&self, uri: impl Into<String>) -> Call<'_> {
        Call(self.client.post(uri))
    }

    pub fn put(&self, uri: impl Into<String>) -> Call<'_> {
        Call(self.client.put(uri))
    }

    pub fn patch(&self, uri: impl Into<String>) -> Call<'_> {
        Call(self.client.patch(uri))
    }

    pub fn delete(&self, uri: impl Into<String>) -> Call<'_> {
        Call(self.client.delete(uri))
    }
}

pub struct Call<'a>(TestRequestBuilder<'a, BoxEndpoint<'static>>);

impl Call<'_> {
    pub fn json(self, body: &Value) -> Self {
        Self(self.0.body_json(body))
    }

    pub fn query(self, name: &str, value: impl Display) -> Self {
        Self(self.0.query(name, &value.to_string()))
    }

    pub fn queries(self, pairs: &[(&str, String)]) -> Self {
        pairs
            .iter()
            .fold(self, |call, (name, value)| call.query(name, value))
    }

    pub fn header(self, name: &'static str, value: impl Display) -> Self {
        Self(self.0.header(name, value.to_string()))
    }

    pub fn if_match(self, version: i64) -> Self {
        self.header("If-Match", format!("\"{version}\""))
    }

    pub fn bearer(self, token: impl Display) -> Self {
        self.header("Authorization", format!("Bearer {token}"))
    }

    /// The body as `T`, or the refusal as the driver's `R`. Panics on a refusal
    /// without the one error shape, whatever raised it.
    pub async fn send<T: DeserializeOwned, R: From<Refused>>(self) -> Result<T, R> {
        let response = self.0.send().await.0;
        if response.status().is_success() {
            return Ok(response
                .into_body()
                .into_json()
                .await
                .expect("the response body has the expected shape"));
        }
        Err(refused(response).await.into())
    }

    /// For an operation that answers with no body, like a delete.
    pub async fn send_empty<R: From<Refused>>(self) -> Result<(), R> {
        let response = self.0.send().await.0;
        if response.status().is_success() {
            let body = response
                .into_body()
                .into_bytes()
                .await
                .expect("read the body");
            assert!(body.is_empty(), "the response has no body");
            return Ok(());
        }
        Err(refused(response).await.into())
    }
}

async fn refused(response: poem::Response) -> Refused {
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .expect("a refusal carries its request id");
    let error: ErrorShape = response
        .into_body()
        .into_json()
        .await
        .expect("a refusal has the error shape");
    assert_eq!(
        error.request_id, request_id,
        "the body names the request id"
    );
    assert!(!error.message.is_empty(), "a refusal explains itself");
    assert!(error.fields.iter().all(|f| !f.message.is_empty()));
    assert!(
        error.fields.is_empty() || error.code == "validation_error",
        "only a validation error names fields"
    );
    Refused {
        code: error.code,
        paths: error.fields.into_iter().map(|f| f.path).collect(),
    }
}

/// A checked refusal: the driver maps `code` onto its own `Refusal`.
#[derive(Debug)]
pub struct Refused {
    pub code: String,
    /// The inputs at fault; only a `validation_error` names any.
    pub paths: Vec<String>,
}

/// Parsed independently of [`crate::ErrorBody`], as a client would, so a change to
/// the wire shape fails here.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ErrorShape {
    code: String,
    message: String,
    #[serde(default)]
    fields: Vec<FieldShape>,
    request_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldShape {
    path: String,
    message: String,
}

/// A page of a list response (`ListResult`), its items read through the driver's
/// JSON newtype.
#[derive(Debug, Deserialize)]
pub struct Page<V> {
    pub items: Vec<V>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

impl<V: Deref<Target = Value>> Page<V> {
    pub fn ids(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|item| id_of(item).to_owned())
            .collect()
    }

    pub fn item(&self, id: impl Display) -> &V {
        let id = id.to_string();
        self.items
            .iter()
            .find(|item| id_of(item) == id)
            .expect("the page holds the item")
    }
}

fn id_of(item: &Value) -> &str {
    item["id"].as_str().expect("an item has an id")
}

/// Declares a driver's newtype over a JSON response body: compared whole with
/// `==`, read through `Deref<Target = Value>`, and built by the driver from an
/// aggregate or published record with `Self(json!(..))`.
#[macro_export]
macro_rules! json_newtype {
    ($(#[$meta:meta])* $vis:vis $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, PartialEq)]
        $vis struct $name($crate::testing::__private::Value);

        impl $name {
            pub fn id(&self) -> &str {
                self.0["id"].as_str().expect(concat!(stringify!($name), " has an id"))
            }

            /// The version to send back in `If-Match`, for a versioned resource.
            pub fn version(&self) -> i64 {
                self.0["version"]
                    .as_str()
                    .and_then(|version| version.parse().ok())
                    .expect(concat!(stringify!($name), " has a version"))
            }
        }

        impl ::std::ops::Deref for $name {
            type Target = $crate::testing::__private::Value;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<'de> $crate::testing::__private::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
            where
                D: $crate::testing::__private::Deserializer<'de>,
            {
                <$crate::testing::__private::Value as $crate::testing::__private::Deserialize>::deserialize(
                    deserializer,
                )
                .map(Self)
            }
        }
    };
}

#[doc(hidden)]
pub mod __private {
    pub use serde::{Deserialize, Deserializer};
    pub use serde_json::Value;
}
