//! The only place in these tests that knows the transport (spec §13).

use app_web::testing::{Call, Page, Refused, TestApi};
use app_web::{Accept, JwtVerifier, json_newtype};
use poem::EndpointExt;
use serde_json::json;
use sqlx::PgPool;
use std::fmt::Display;
use std::sync::Arc;
use users::{User, UserId, UsersEndpoints, seed};
use users_api::UserDirectory;

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A rule was broken: the paths of the inputs at fault, if it names any.
    Invalid(Vec<String>),
    /// The request couldn't be parsed.
    Malformed,
    NotSignedIn,
    NotFound,
    Conflict,
    PreconditionRequired,
    /// The `If-Match` version was stale.
    Stale,
}

impl Refusal {
    pub fn invalid<const N: usize>(fields: [&str; N]) -> Self {
        Refusal::Invalid(fields.map(String::from).to_vec())
    }
}

impl From<Refused> for Refusal {
    fn from(refused: Refused) -> Self {
        match refused.code.as_str() {
            "validation_error" => Refusal::Invalid(refused.paths),
            "malformed_request" => Refusal::Malformed,
            "unauthorized" => Refusal::NotSignedIn,
            "not_found" => Refusal::NotFound,
            "conflict" => Refusal::Conflict,
            "precondition_required" => Refusal::PreconditionRequired,
            "precondition_failed" => Refusal::Stale,
            other => panic!("unexpected error code {other}"),
        }
    }
}

pub type Outcome<T> = Result<T, Refusal>;

json_newtype!(pub UserJson);

impl From<User> for UserJson {
    /// Version `1`: seeded and freshly-registered users are always at it.
    fn from(u: User) -> Self {
        Self(json!({
            "id": u.id().to_string(),
            "email": u.email().as_str(),
            "displayName": u.display_name(),
            "createdAt": u.created_at().to_rfc3339(),
            "version": "1",
        }))
    }
}

pub type UsersPage = Page<UserJson>;

pub struct Users {
    api: TestApi,
    verifier: JwtVerifier,
}

impl Users {
    pub async fn seeded(pool: PgPool) -> Self {
        migrate_and_seed(&pool).await;

        let verifier = JwtVerifier::new("test-secret");
        let api = TestApi::new(UsersEndpoints, |app| {
            app.accept(users::provider(pool)).data(verifier.clone())
        });
        Self { api, verifier }
    }

    pub async fn register(&self, email: &str, display_name: &str) -> Outcome<UserJson> {
        let request = json!({ "email": email, "displayName": display_name });
        self.api.post("/api/users").json(&request).send().await
    }

    pub async fn get(&self, id: impl Display) -> Outcome<UserJson> {
        self.api.get(format!("/api/users/{id}")).send().await
    }

    pub async fn list(&self) -> UsersPage {
        self.list_with(&[])
            .await
            .expect("listing users never fails")
    }

    /// `sort` keys as the API names them, `-` prefix for descending.
    pub async fn list_sorted(&self, sort: &[&str]) -> Outcome<UsersPage> {
        let query: Vec<_> = sort.iter().map(|key| ("sort", key.to_string())).collect();
        self.list_with(&query).await
    }

    pub async fn list_page(&self, limit: i64, offset: i64) -> UsersPage {
        self.list_with(&[("limit", limit.to_string()), ("offset", offset.to_string())])
            .await
            .expect("listing users never fails")
    }

    async fn list_with(&self, query: &[(&str, String)]) -> Outcome<UsersPage> {
        self.api.get("/api/users").queries(query).send().await
    }

    /// The user need not exist.
    pub fn signed_in_as(&self, user: UserId) -> Session<'_> {
        let token = self
            .verifier
            .issue(user.to_string(), 60)
            .expect("issue token");
        Session {
            users: self,
            token: Some(token),
        }
    }

    pub fn anonymous(&self) -> Session<'_> {
        Session {
            users: self,
            token: None,
        }
    }
}

pub struct Session<'a> {
    users: &'a Users,
    token: Option<String>,
}

impl Session<'_> {
    pub async fn me(&self) -> Outcome<UserJson> {
        self.signed(self.users.api.get("/api/users/me"))
            .send()
            .await
    }

    pub async fn change_display_name(&self, version: i64, display_name: &str) -> Outcome<UserJson> {
        self.signed(self.users.api.patch("/api/users/me"))
            .if_match(version)
            .json(&json!({ "displayName": display_name }))
            .send()
            .await
    }

    pub async fn change_display_name_without_precondition(
        &self,
        display_name: &str,
    ) -> Outcome<UserJson> {
        self.signed(self.users.api.patch("/api/users/me"))
            .json(&json!({ "displayName": display_name }))
            .send()
            .await
    }

    fn signed<'c>(&self, call: Call<'c>) -> Call<'c> {
        match &self.token {
            Some(token) => call.bearer(token),
            None => call,
        }
    }
}

/// Tested directly: the directory has no HTTP entry point.
pub async fn seeded_directory(pool: PgPool) -> Arc<dyn UserDirectory> {
    migrate_and_seed(&pool).await;
    users::directory(pool)
}

async fn migrate_and_seed(pool: &PgPool) {
    app_db::run_migrations(pool, &[users::migrations()])
        .await
        .expect("migrate users");
    seed::standard(pool).await.expect("seed users");
}
