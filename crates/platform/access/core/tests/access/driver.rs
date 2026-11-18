//! The only place in these tests that knows the transport (ADR 0004).

use crate::workshop::{self, Release};
use access::{AccessEndpoints, Rules};
use app_authz::{Plan, Principal};
use app_db::Worker;
use app_domain::{FixedClock, seed_time};
use app_events::{Event, PayloadOf};
use app_module::Context;
use app_persistence::{Handlers, Version};
use app_test_db::Template;
use app_web::testing::{Call, Page, Refused, TestApi};
use app_web::{Accept, JwtVerifier, json_newtype};
use poem::EndpointExt;
use serde_json::json;
use sqlx::PgPool;
use std::fmt::Display;
use std::sync::{Arc, Mutex};

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A rule was broken: the paths of the inputs at fault, if it names any.
    Invalid(Vec<String>),
    NotSignedIn,
    /// Signed in, but the policies don't allow it.
    Forbidden,
    NotFound,
    Conflict,
    PreconditionRequired,
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
            "unauthorized" => Refusal::NotSignedIn,
            "forbidden" => Refusal::Forbidden,
            "not_found" => Refusal::NotFound,
            "conflict" => Refusal::Conflict,
            "precondition_required" => Refusal::PreconditionRequired,
            "precondition_failed" => Refusal::Stale,
            other => panic!("unexpected error code {other}"),
        }
    }
}

pub type Outcome<T> = Result<T, Refusal>;

json_newtype!(pub GrantJson);

impl GrantJson {
    /// `(role, resource, action, scope)`.
    pub fn rule(&self) -> (String, String, String, String) {
        let field = |name: &str| self[name].as_str().unwrap_or_default().to_owned();
        (
            field("role"),
            field("resource"),
            field("action"),
            field("scope"),
        )
    }
}

json_newtype!(pub CatalogJson);

json_newtype!(pub RoleJson);

impl RoleJson {
    pub fn name(&self) -> &str {
        self["name"].as_str().unwrap_or_default()
    }
}

pub type Roles = Page<RoleJson>;

pub type Grants = Page<GrantJson>;

/// Who administers the app in these tests.
pub const ADMIN_ID: &str = "an-admin";
pub const ADMIN: &str = "admin";

/// One server instance of `release`, over the test's database.
pub struct Access {
    api: TestApi,
    ctx: Context,
    rules: Rules,
    release: Release,
    worker: Worker,
    seeded_through: i64,
    verifier: JwtVerifier,
    /// The token later requests present, if any.
    caller: Mutex<Option<String>>,
}

impl Access {
    /// Of the first release, as `migrate` left it, serving once it has read the grants.
    pub async fn seeded(pool: PgPool) -> Self {
        Self::of(pool, Release::First).await
    }

    pub async fn of(pool: PgPool, release: Release) -> Self {
        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let modules = Arc::new(workshop::modules(release));
        let authorizer =
            app_authz::Authorizer::baseline(&modules.policies).expect("the policies compile");
        let ctx = Context::new(pool, clock).with_authorizer(Arc::new(authorizer));
        let rules = access::rules(&ctx, modules.clone());
        rules.refresh().await.expect("the grants are read");
        let verifier = JwtVerifier::new("test-secret");
        let api = TestApi::new(AccessEndpoints, ctx.idempotency(), |app| {
            app.accept(access::provider(&ctx, modules))
                .data(verifier.clone())
                .data(ctx.authorizer())
        });
        let worker = ctx.worker(Handlers::new());
        let seeded_through = worker.outbox_end().await.expect("read the outbox");
        let access = Self {
            api,
            ctx,
            rules,
            release,
            worker,
            seeded_through,
            verifier,
            caller: Mutex::new(None),
        };
        access.sign_in_with_roles(&[ADMIN]);
        access
    }

    /// `migrate` of this instance's release: how many default grants it granted.
    pub async fn migrate(&self) -> usize {
        access::apply_default_grants(&self.ctx, Arc::new(workshop::modules(self.release)))
            .await
            .expect("the defaults are applied")
    }

    /// The instance's next poll of the revision.
    pub async fn polls(&self) {
        self.rules.refresh().await.expect("the grants are read");
    }

    /// Whether the rule set in force lets a holder of `role` do `action` to every `resource`.
    pub fn allows(&self, role: &str, resource: &str, action: &str) -> bool {
        self.ctx.authorizer().current().plan_named(
            &Principal::user("someone", [role]),
            resource,
            action,
        ) == Plan::Allowed
    }

    pub async fn revision(&self) -> i64 {
        sqlx::query_scalar("SELECT revision FROM access.revision")
            .fetch_one(&self.ctx.pool())
            .await
            .expect("read the revision")
    }

    /// Every `E` announced since the instance started, oldest first, with its grant's version.
    pub async fn announced<E: Event>(&self) -> Vec<(Version, PayloadOf<E>)> {
        self.worker
            .published_after::<E>(self.seeded_through)
            .await
            .expect("read the outbox")
            .into_iter()
            .map(|event| (Version::from_i64(event.entity_version), event.data))
            .collect()
    }

    /// Later requests present a real token of `ADMIN_ID` carrying `roles`. Every test starts
    /// signed in as an `admin`.
    pub fn sign_in_with_roles(&self, roles: &[&str]) {
        let token = self
            .verifier
            .issue(ADMIN_ID, roles, 60)
            .expect("issue token");
        *self.caller.lock().unwrap() = Some(token);
    }

    pub fn sign_out(&self) {
        *self.caller.lock().unwrap() = None;
    }

    fn signed<'c>(&self, call: Call<'c>) -> Call<'c> {
        match self.caller.lock().unwrap().as_ref() {
            Some(token) => call.bearer(token),
            None => call,
        }
    }

    /// Every grant, in the default order.
    pub async fn grants(&self) -> Outcome<Grants> {
        self.grants_where(&[]).await
    }

    /// `query` as the API names its parameters: `role`, `resource`, `action`, `sort`.
    pub async fn grants_where(&self, query: &[(&str, &str)]) -> Outcome<Grants> {
        let query: Vec<_> = query.iter().map(|(k, v)| (*k, v.to_string())).collect();
        self.signed(self.api.get("/api/access/grants").queries(&query))
            .send()
            .await
    }

    /// The grant of `(role, resource, action, scope)`, which must exist.
    pub async fn grant_of(&self, rule: (&str, &str, &str, &str)) -> GrantJson {
        self.grants()
            .await
            .expect("an admin lists the grants")
            .items
            .into_iter()
            .find(|g| {
                let (role, resource, action, scope) = g.rule();
                (
                    role.as_str(),
                    resource.as_str(),
                    action.as_str(),
                    scope.as_str(),
                ) == rule
            })
            .unwrap_or_else(|| panic!("{rule:?} is granted"))
    }

    pub async fn grant(
        &self,
        role: &str,
        resource: &str,
        action: &str,
        scope: &str,
    ) -> Outcome<GrantJson> {
        self.signed(self.api.post("/api/access/grants"))
            .json(&json!({ "role": role, "resource": resource, "action": action, "scope": scope }))
            .send()
            .await
    }

    pub async fn revoke(&self, grant: &GrantJson) -> Outcome<()> {
        self.revoke_at(grant.id(), grant.version()).await
    }

    pub async fn revoke_at(&self, id: impl Display, version: i64) -> Outcome<()> {
        self.signed(self.api.delete(format!("/api/access/grants/{id}")))
            .if_match(version)
            .send_empty()
            .await
    }

    pub async fn revoke_without_precondition(&self, grant: &GrantJson) -> Outcome<()> {
        self.signed(
            self.api
                .delete(format!("/api/access/grants/{}", grant.id())),
        )
        .send_empty()
        .await
    }

    /// Every role, by name.
    pub async fn roles(&self) -> Outcome<Roles> {
        self.signed(self.api.get("/api/access/roles")).send().await
    }

    pub async fn role_names(&self) -> Vec<String> {
        self.roles()
            .await
            .expect("an admin lists the roles")
            .items
            .iter()
            .map(|r| r.name().to_owned())
            .collect()
    }

    /// The role named `name`, which must exist.
    pub async fn role(&self, name: &str) -> RoleJson {
        self.roles()
            .await
            .expect("an admin lists the roles")
            .items
            .into_iter()
            .find(|r| r.name() == name)
            .unwrap_or_else(|| panic!("a role is named {name}"))
    }

    pub async fn create_role(&self, name: &str, description: &str) -> Outcome<RoleJson> {
        self.signed(self.api.post("/api/access/roles"))
            .json(&json!({ "name": name, "description": description }))
            .send()
            .await
    }

    pub async fn delete_role(&self, role: &RoleJson) -> Outcome<()> {
        self.delete_role_at(role.name(), role.version()).await
    }

    pub async fn delete_role_at(&self, name: &str, version: i64) -> Outcome<()> {
        self.signed(self.api.delete(format!("/api/access/roles/{name}")))
            .if_match(version)
            .send_empty()
            .await
    }

    pub async fn delete_role_without_precondition(&self, role: &RoleJson) -> Outcome<()> {
        self.signed(
            self.api
                .delete(format!("/api/access/roles/{}", role.name())),
        )
        .send_empty()
        .await
    }

    pub async fn catalog(&self) -> Outcome<CatalogJson> {
        self.signed(self.api.get("/api/access/catalog"))
            .send()
            .await
    }
}

/// Every test's database starts as a copy of this: migrated, and the first release's default
/// grants applied, as `migrate` leaves it.
pub const TEMPLATE: Template = Template::new("access", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &[access::migrations()])
            .await
            .expect("migrate access");
        let ctx = Context::new(pool, Arc::new(app_domain::SystemClock));
        access::apply_default_grants(&ctx, Arc::new(workshop::modules(Release::First)))
            .await
            .expect("apply the defaults");
    })
});
