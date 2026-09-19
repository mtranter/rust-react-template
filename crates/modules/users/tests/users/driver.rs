//! Test driver: the users module's behaviour as domain verbs and outcomes. The
//! only place in these tests that knows the transport (routes, JSON, statuses,
//! bearer tokens).

use app_web::{Accept, JwtVerifier};
use poem::http::StatusCode;
use poem::test::{TestClient, TestResponse};
use poem::{EndpointExt, Route, endpoint::BoxEndpoint, middleware::AddData};
use poem_openapi::OpenApiService;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::fmt::Display;
use std::ops::Deref;
use users::{User, UserId, UsersApi, seed};

/// Why the module refused a request.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The input broke a rule (e.g. a malformed email).
    Invalid,
    /// The caller is not signed in.
    NotSignedIn,
    /// The thing asked for does not exist.
    NotFound,
    /// The request clashes with existing state (e.g. an email already taken).
    Conflict,
    /// A change arrived without the required `If-Match` version.
    PreconditionRequired,
    /// The `If-Match` version was stale — the account changed underneath the caller.
    Stale,
}

pub type Outcome<T> = Result<T, Refusal>;

/// A user as a client sees them. Wraps the returned JSON: compare it whole
/// against a seed (`Ok(seed::alice().into())`), or assert on parts of it by path
/// (`assert_json_matches!(user, "$.email", "carol@example.com")`) when some
/// properties, like a generated id, can't be predicted.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(transparent)]
pub struct UserView(Value);

impl UserView {
    pub fn id(&self) -> &str {
        self.0["id"].as_str().expect("a user has an id")
    }
}

impl Deref for UserView {
    type Target = Value;
    fn deref(&self) -> &Value {
        &self.0
    }
}

impl From<User> for UserView {
    /// Builds the expected view of a user. Seeded and freshly-registered users
    /// are always at version `1`, so that is what a plain `User` maps to; changed
    /// users are compared against the change's response, which carries its own.
    fn from(u: User) -> Self {
        Self(json!({
            "id": u.id().to_string(),
            "email": u.email().as_str(),
            "display_name": u.display_name(),
            "created_at": u.created_at().to_rfc3339(),
            "version": "1",
        }))
    }
}

/// One page of users plus the total across all pages.
#[derive(Debug, Deserialize)]
pub struct UsersPage {
    pub items: Vec<UserView>,
    pub total: i64,
}

impl UsersPage {
    pub fn ids(&self) -> Vec<String> {
        self.items.iter().map(|u| u.id().to_string()).collect()
    }
}

pub struct Users {
    client: TestClient<BoxEndpoint<'static>>,
    verifier: JwtVerifier,
}

impl Users {
    /// The users module running over `pool`, migrated and holding the standard
    /// seed dataset.
    pub async fn seeded(pool: PgPool) -> Self {
        app_db::run_migrations(&pool, &[users::migrations()])
            .await
            .expect("migrate users");
        seed::standard(&pool).await.expect("seed users");

        let verifier = JwtVerifier::new("test-secret");
        let api = OpenApiService::new(UsersApi, "users", "1.0");
        let app = Route::new()
            .nest("/api", api)
            .accept(users::provider(pool))
            .with(AddData::new(verifier.clone()))
            .boxed();
        Self {
            client: TestClient::new(app),
            verifier,
        }
    }

    pub async fn register(&self, email: &str, display_name: &str) -> Outcome<UserView> {
        let request = json!({ "email": email, "display_name": display_name });
        outcome(
            self.client
                .post("/api/users")
                .body_json(&request)
                .send()
                .await,
        )
        .await
    }

    pub async fn get(&self, id: impl Display) -> Outcome<UserView> {
        outcome(self.client.get(format!("/api/users/{id}")).send().await).await
    }

    pub async fn list(&self) -> UsersPage {
        outcome(self.client.get("/api/users").send().await)
            .await
            .expect("listing users never fails")
    }

    /// Act as the given user (who need not exist).
    pub fn signed_in_as(&self, user: UserId) -> Session<'_> {
        let token = self
            .verifier
            .issue(user.to_string(), 60)
            .expect("issue token");
        Session {
            users: self,
            bearer: Some(format!("Bearer {token}")),
        }
    }

    /// Act without signing in.
    pub fn anonymous(&self) -> Session<'_> {
        Session {
            users: self,
            bearer: None,
        }
    }
}

/// Requests made on behalf of one caller.
pub struct Session<'a> {
    users: &'a Users,
    bearer: Option<String>,
}

impl Session<'_> {
    pub async fn me(&self) -> Outcome<UserView> {
        let mut request = self.users.client.get("/api/users/me");
        if let Some(bearer) = &self.bearer {
            request = request.header("Authorization", bearer);
        }
        outcome(request.send().await).await
    }

    /// Change the display name, asserting the account is still at `version` (the
    /// `If-Match` precondition).
    pub async fn change_display_name(&self, version: i64, display_name: &str) -> Outcome<UserView> {
        let mut request = self
            .users
            .client
            .patch("/api/users/me")
            .header("If-Match", format!("\"{version}\""))
            .body_json(&json!({ "display_name": display_name }));
        if let Some(bearer) = &self.bearer {
            request = request.header("Authorization", bearer);
        }
        outcome(request.send().await).await
    }

    /// Change the display name without stating which version — the
    /// missing-precondition path.
    pub async fn change_display_name_without_precondition(
        &self,
        display_name: &str,
    ) -> Outcome<UserView> {
        let mut request = self
            .users
            .client
            .patch("/api/users/me")
            .body_json(&json!({ "display_name": display_name }));
        if let Some(bearer) = &self.bearer {
            request = request.header("Authorization", bearer);
        }
        outcome(request.send().await).await
    }
}

/// Translate a response into a domain outcome.
async fn outcome<T: DeserializeOwned>(response: TestResponse) -> Outcome<T> {
    match response.0.status() {
        StatusCode::OK => Ok(response
            .0
            .into_body()
            .into_json()
            .await
            .expect("response body matches the expected shape")),
        StatusCode::BAD_REQUEST => Err(Refusal::Invalid),
        StatusCode::UNAUTHORIZED => Err(Refusal::NotSignedIn),
        StatusCode::NOT_FOUND => Err(Refusal::NotFound),
        StatusCode::CONFLICT => Err(Refusal::Conflict),
        StatusCode::PRECONDITION_REQUIRED => Err(Refusal::PreconditionRequired),
        StatusCode::PRECONDITION_FAILED => Err(Refusal::Stale),
        other => panic!("unexpected status {other}"),
    }
}
