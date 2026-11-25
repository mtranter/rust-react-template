//! The only place in these tests that knows the transport (ADR 0004).

use crate::workshop::{self, Release};
use access::{
    AccessEndpoints, Created, DirectoryStatus, DirectoryUser, Email, IdentityDirectory, Rules,
};
use app_authz::{Plan, Principal};
use app_db::Worker;
use app_domain::{FixedClock, seed_time};
use app_error::{AppError, AppResult};
use app_events::{Event, PayloadOf};
use app_module::Context;
use app_persistence::{Handlers, Version};
use app_test_db::Template;
use app_web::testing::{Call, Page, Refused, TestApi};
use app_web::{Accept, Claims, JwtVerifier, json_newtype};
use async_trait::async_trait;
use chrono::TimeDelta;
use poem::EndpointExt;
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;
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
    /// The identity provider can't be reached: retry later.
    Unavailable,
    /// No identity provider is configured.
    NotConfigured,
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
            "unavailable" => Refusal::Unavailable,
            "not_configured" => Refusal::NotConfigured,
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

impl CatalogJson {
    /// Every Capability's resources, in the catalog's order.
    pub fn resources(&self) -> Vec<&serde_json::Value> {
        self["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|c| c["resources"].as_array().unwrap())
            .collect()
    }
}

json_newtype!(pub PermissionsJson);

impl PermissionsJson {
    /// `(resource, action)`, in the order answered.
    pub fn pairs(&self) -> Vec<(String, String)> {
        self["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p["resource"].as_str().unwrap().to_owned(),
                    p["action"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }
}

json_newtype!(pub RoleJson);

json_newtype!(pub AssignmentJson);

impl RoleJson {
    pub fn name(&self) -> &str {
        self["name"].as_str().unwrap_or_default()
    }
}

pub type Roles = Page<RoleJson>;

json_newtype!(pub SubjectJson);

impl SubjectJson {
    pub fn subject(&self) -> &str {
        self["subject"].as_str().unwrap_or_default()
    }

    /// The names of the roles it holds beside `user`.
    pub fn roles(&self) -> Vec<String> {
        self["roles"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| r["role"].as_str().unwrap_or_default().to_owned())
            .collect()
    }
}

pub type Subjects = Page<SubjectJson>;

pub type Grants = Page<GrantJson>;

json_newtype!(pub InvitationJson);

impl InvitationJson {
    pub fn subject(&self) -> &str {
        self["subject"].as_str().unwrap_or_default()
    }
}

pub type Invitations = Page<InvitationJson>;

/// A user the fake identity provider holds.
#[derive(Debug, Clone)]
struct DirectoryEntry {
    subject: String,
    name: String,
    status: DirectoryStatus,
}

/// Stands in for Cognito, which tests can't run: keeps its accounts by email, counts the invitation
/// mails it would send, and refuses everything while it's down.
#[derive(Default)]
pub struct FakeDirectory {
    accounts: Mutex<HashMap<String, DirectoryEntry>>,
    mailed: Mutex<Vec<String>>,
    down: std::sync::atomic::AtomicBool,
    next: std::sync::atomic::AtomicU32,
}

impl FakeDirectory {
    fn up(&self) -> AppResult<()> {
        if self.down.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(AppError::unavailable(
                "the identity provider can't be reached",
            ));
        }
        Ok(())
    }

    fn found(&self, email: &Email) -> DirectoryUser {
        match self.accounts.lock().unwrap().get(email.as_str()) {
            Some(entry) => DirectoryUser {
                subject: entry.subject.clone(),
                status: entry.status,
            },
            None => DirectoryUser {
                subject: String::new(),
                status: DirectoryStatus::Missing,
            },
        }
    }
}

#[async_trait]
impl IdentityDirectory for FakeDirectory {
    async fn create(&self, email: &Email, name: &str) -> AppResult<Created> {
        self.up()?;
        if email.as_str().ends_with("@refused.example") {
            return Err(AppError::invalid_field(
                "email",
                "the identity provider refused it",
            ));
        }
        let mut accounts = self.accounts.lock().unwrap();
        if let Some(entry) = accounts.get(email.as_str()) {
            return Ok(Created::Exists(DirectoryUser {
                subject: entry.subject.clone(),
                status: entry.status,
            }));
        }
        let n = self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let entry = DirectoryEntry {
            subject: format!("sub-{n}"),
            name: name.to_owned(),
            status: DirectoryStatus::Pending,
        };
        accounts.insert(email.as_str().to_owned(), entry.clone());
        self.mailed.lock().unwrap().push(email.as_str().to_owned());
        Ok(Created::Created(DirectoryUser {
            subject: entry.subject,
            status: entry.status,
        }))
    }

    async fn resend(&self, email: &Email) -> AppResult<()> {
        self.up()?;
        if let Some(entry) = self.accounts.lock().unwrap().get_mut(email.as_str()) {
            entry.status = DirectoryStatus::Pending;
        }
        self.mailed.lock().unwrap().push(email.as_str().to_owned());
        Ok(())
    }

    async fn find(&self, email: &Email) -> AppResult<DirectoryUser> {
        self.up()?;
        Ok(self.found(email))
    }

    async fn delete(&self, email: &Email) -> AppResult<()> {
        self.up()?;
        self.accounts.lock().unwrap().remove(email.as_str());
        Ok(())
    }
}

/// Who administers the app in these tests: the template assigns them `admin`, the only subject
/// it is assigned to.
pub const ADMIN_ID: &str = "an-admin";
pub const ADMIN: &str = "admin";

/// One server instance of `release`, over the test's database.
pub struct Access {
    api: TestApi,
    ctx: Context,
    clock: Arc<FixedClock>,
    rules: Rules,
    release: Release,
    worker: Worker,
    seeded_through: i64,
    verifier: JwtVerifier,
    /// The token later requests present, if any.
    caller: Mutex<Option<String>>,
    directory: Arc<FakeDirectory>,
}

impl Access {
    /// Of the first release, as `migrate` left it, serving once it has read the grants.
    pub async fn seeded(pool: PgPool) -> Self {
        Self::of(pool, Release::First).await
    }

    pub async fn of(pool: PgPool, release: Release) -> Self {
        Self::with_bootstrap_admins(pool, release, &[]).await
    }

    /// With `subjects` as `APP_BOOTSTRAP_ADMINS`.
    pub async fn with_bootstrap_admins(pool: PgPool, release: Release, subjects: &[&str]) -> Self {
        Self::built(pool, release, subjects, true).await
    }

    /// With no identity provider configured: inviting is off.
    pub async fn without_a_directory(pool: PgPool) -> Self {
        Self::built(pool, Release::First, &[], false).await
    }

    async fn built(pool: PgPool, release: Release, subjects: &[&str], directory: bool) -> Self {
        let clock = Arc::new(FixedClock::new(seed_time(24 * 60)));
        let modules = Arc::new(workshop::modules(release));
        let authorizer = app_authz::Authorizer::baseline(&modules.policies)
            .expect("the policies compile")
            .with_bootstrap_admins(subjects.iter().map(|s| (*s).to_owned()));
        let ctx = Context::new(pool, clock.clone()).with_authorizer(Arc::new(authorizer));
        let rules = access::rules(&ctx, modules.clone());
        rules.refresh().await.expect("the grants are read");
        let verifier = JwtVerifier::new("test-secret");
        let fake = Arc::new(FakeDirectory::default());
        let configured: Option<Arc<dyn IdentityDirectory>> = directory.then(|| fake.clone() as _);
        let api = TestApi::new(AccessEndpoints, ctx.idempotency(), |app| {
            app.accept(access::provider(&ctx, modules, configured))
                .data(access::sign_ins(&ctx))
                .data(verifier.clone())
                .data(ctx.authorizer())
        });
        let worker = ctx.worker(Handlers::new());
        let seeded_through = worker.outbox_end().await.expect("read the outbox");
        let access = Self {
            api,
            ctx,
            clock,
            rules,
            release,
            worker,
            seeded_through,
            verifier,
            caller: Mutex::new(None),
            directory: fake,
        };
        access.sign_in_as(ADMIN_ID);
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

    /// The pool closed: whatever reads the database fails from now on.
    pub async fn database_goes_away(&self) {
        self.ctx.pool().close().await;
    }

    /// The database as a build before ADR 0047 left it: `migrate` has not retired the gates, and
    /// each `(role, module)` of `holders` is granted that Module's gate, `module:<module>` / `use`,
    /// the role created if it doesn't exist. Written as rows, since `access` now refuses gates.
    pub async fn had_gates(&self, holders: &[(&str, &str)]) {
        let pool = self.ctx.pool();
        sqlx::query("DELETE FROM access.gates_retired")
            .execute(&pool)
            .await
            .expect("forget the gates' retirement");
        for (role, module) in holders {
            sqlx::query(
                "INSERT INTO access.role (name, description, created_at) VALUES ($1, '', now())
                 ON CONFLICT (name) DO NOTHING",
            )
            .bind(role)
            .execute(&pool)
            .await
            .expect("create the role");
            sqlx::query(
                "INSERT INTO access.grant (id, role, resource, action, scope, granted_at)
                 VALUES (gen_random_uuid(), $1, 'module:' || $2, 'use', 'any', now())",
            )
            .bind(role)
            .bind(module)
            .execute(&pool)
            .await
            .expect("grant the gate");
        }
    }

    /// Whether `migrate` has retired the Modules' gates.
    pub async fn gates_retired(&self) -> bool {
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM access.gates_retired)")
            .fetch_one(&self.ctx.pool())
            .await
            .expect("read the gates' retirement")
    }

    /// Every grant stored, gates included, as `(role, resource, action, scope)`, sorted.
    pub async fn stored_grants(&self) -> Vec<(String, String, String, String)> {
        let mut rows: Vec<(String, String, String, String)> =
            sqlx::query_as("SELECT role, resource, action, scope FROM access.grant")
                .fetch_all(&self.ctx.pool())
                .await
                .expect("read the grants");
        rows.sort();
        rows
    }

    /// How the rule set in force plans `action` on every `resource` for a user holding `role`
    /// beside `user`, as every signed-in user does.
    pub fn plan(&self, role: &str, resource: &str, action: &str) -> Plan {
        self.ctx.authorizer().current().plan_named(
            &Principal::user("someone", [app_authz::USER, role]),
            resource,
            action,
        )
    }

    /// Every `E` announced since the database was made, the template's included.
    pub async fn announced_ever<E: Event>(&self) -> Vec<(Version, PayloadOf<E>)> {
        self.worker
            .published_after::<E>(0)
            .await
            .expect("read the outbox")
            .into_iter()
            .map(|event| (Version::from_i64(event.entity_version), event.data))
            .collect()
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

    /// Later requests present a real token of `subject`, holding the roles assigned to it in
    /// the rule set in force. Every test starts signed in as `ADMIN_ID`, an `admin`.
    pub fn sign_in_as(&self, subject: &str) {
        let token = self.verifier.issue(subject, 60).expect("issue token");
        *self.caller.lock().unwrap() = Some(token);
    }

    /// As [`Self::sign_in_as`], with a token carrying the subject's name and email.
    pub fn sign_in_with_profile(&self, subject: &str, name: &str, email: &str) {
        let claims = Claims::lasting(subject, 60)
            .expect("claims")
            .named(name)
            .with_email(email);
        let token = self.verifier.sign(&claims).expect("issue token");
        *self.caller.lock().unwrap() = Some(token);
    }

    /// Later requests present a client-credentials token of `client`, an agent acting for no
    /// one: its subject is its own client.
    pub fn sign_in_as_agent(&self, client: &str) {
        let claims = Claims::lasting(client, 60)
            .expect("claims")
            .from_client(client);
        let token = self.verifier.sign(&claims).expect("issue token");
        *self.caller.lock().unwrap() = Some(token);
    }

    pub fn later(&self, by: TimeDelta) {
        self.clock.advance(by);
    }

    /// A request of the signed-in subject's that `access` itself refuses nothing of.
    pub async fn makes_a_request(&self) {
        let _ = self.roles().await;
    }

    /// The physical version of `subject`'s record (Postgres' `xmin`), which every write to it
    /// changes: `None` if it has none.
    pub async fn subject_record_written(&self, subject: &str) -> Option<String> {
        sqlx::query_scalar("SELECT xmin::text FROM access.subject WHERE subject = $1")
            .bind(subject)
            .fetch_optional(&self.ctx.pool())
            .await
            .expect("read the subject")
    }

    pub async fn subjects(&self) -> Outcome<Subjects> {
        self.subjects_where(&[]).await
    }

    /// `query` as the API names its parameters: `search`, `sort`, `limit`, `offset`.
    pub async fn subjects_where(&self, query: &[(&str, &str)]) -> Outcome<Subjects> {
        let query: Vec<_> = query.iter().map(|(k, v)| (*k, v.to_string())).collect();
        self.signed(self.api.get("/api/access/subjects").queries(&query))
            .send()
            .await
    }

    /// `subject` as an admin lists it, which must be listed.
    pub async fn subject(&self, subject: &str) -> SubjectJson {
        self.subjects()
            .await
            .expect("an admin lists the subjects")
            .items
            .into_iter()
            .find(|s| s.subject() == subject)
            .unwrap_or_else(|| panic!("{subject} is listed"))
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

    pub async fn assign(&self, subject: &str, role: &str) -> Outcome<AssignmentJson> {
        self.signed(self.api.post("/api/access/assignments"))
            .json(&json!({ "subject": subject, "role": role }))
            .send()
            .await
    }

    pub async fn unassign(&self, assignment: &AssignmentJson) -> Outcome<()> {
        let field = |name: &str| assignment[name].as_str().unwrap_or_default().to_owned();
        self.unassign_at(&field("subject"), &field("role"), assignment.version())
            .await
    }

    pub async fn unassign_at(&self, subject: &str, role: &str, version: i64) -> Outcome<()> {
        self.signed(
            self.api
                .delete("/api/access/assignments")
                .query("subject", subject)
                .query("role", role),
        )
        .if_match(version)
        .send_empty()
        .await
    }

    pub async fn unassign_without_precondition(&self, subject: &str, role: &str) -> Outcome<()> {
        self.signed(
            self.api
                .delete("/api/access/assignments")
                .query("subject", subject)
                .query("role", role),
        )
        .send_empty()
        .await
    }

    /// The roles `subject` holds on this instance, from the rule set in force.
    pub fn roles_of(&self, subject: &str) -> Vec<String> {
        match self.ctx.authorizer().principal(subject) {
            Principal::User { roles, .. } => roles,
            Principal::System => unreachable!("a subject resolves to a user"),
        }
    }

    /// What the signed-in caller may do, as `(resource, action)`.
    pub async fn my_permissions(&self) -> Outcome<Vec<(String, String)>> {
        self.signed(self.api.get("/api/access/me/permissions"))
            .send()
            .await
            .map(|p: PermissionsJson| p.pairs())
    }

    pub async fn invite(&self, email: &str, name: &str, roles: &[&str]) -> Outcome<InvitationJson> {
        self.signed(self.api.post("/api/access/invitations"))
            .json(&json!({ "email": email, "name": name, "roles": roles }))
            .send()
            .await
    }

    pub async fn invitations(&self) -> Outcome<Invitations> {
        self.invitations_where(&[]).await
    }

    /// `query` as the API names its parameters: `status`, `sort`, `limit`, `offset`.
    pub async fn invitations_where(&self, query: &[(&str, &str)]) -> Outcome<Invitations> {
        let query: Vec<_> = query.iter().map(|(k, v)| (*k, v.to_string())).collect();
        self.signed(self.api.get("/api/access/invitations").queries(&query))
            .send()
            .await
    }

    /// The invitation as an admin now lists it, which must be listed.
    pub async fn invitation(&self, invitation: &InvitationJson) -> InvitationJson {
        self.invitations()
            .await
            .expect("an admin lists the invitations")
            .items
            .into_iter()
            .find(|i| i.id() == invitation.id())
            .expect("the invitation is listed")
    }

    pub async fn resend(&self, invitation: &InvitationJson) -> Outcome<InvitationJson> {
        self.signed(self.api.post(format!(
            "/api/access/invitations/{}/resend",
            invitation.id()
        )))
        .if_match(invitation.version())
        .send()
        .await
    }

    pub async fn revoke_invitation(&self, invitation: &InvitationJson) -> Outcome<()> {
        self.revoke_invitation_at(invitation.id(), invitation.version())
            .await
    }

    pub async fn revoke_invitation_at(&self, id: &str, version: i64) -> Outcome<()> {
        self.signed(self.api.delete(format!("/api/access/invitations/{id}")))
            .if_match(version)
            .send_empty()
            .await
    }

    /// The invitee, `subject`, makes their first request: signed in at the identity provider.
    pub async fn invitee_signs_in(&self, subject: &str) {
        self.sign_in_as(subject);
        self.makes_a_request().await;
        self.sign_in_as(ADMIN_ID);
    }

    /// The identity provider, as an admin would see it in its console.
    pub fn directory_has(&self, email: &str) -> Option<DirectoryStatus> {
        self.directory
            .accounts
            .lock()
            .unwrap()
            .get(email)
            .map(|entry| entry.status)
    }

    pub fn directory_name_of(&self, email: &str) -> Option<String> {
        self.directory
            .accounts
            .lock()
            .unwrap()
            .get(email)
            .map(|entry| entry.name.clone())
    }

    /// The invitation mails the identity provider sent, to whom, oldest first.
    pub fn mailed(&self) -> Vec<String> {
        self.directory.mailed.lock().unwrap().clone()
    }

    /// Someone the identity provider has, created outside the app, in `status`.
    pub fn directory_holds(&self, email: &str, subject: &str, status: DirectoryStatus) {
        self.directory.accounts.lock().unwrap().insert(
            email.to_owned(),
            DirectoryEntry {
                subject: subject.to_owned(),
                name: String::new(),
                status,
            },
        );
    }

    /// The invitee's account changes at the identity provider: they set a password, their
    /// temporary one lapses, or an admin deletes them in its console.
    pub fn directory_sets(&self, email: &str, status: Option<DirectoryStatus>) {
        let mut accounts = self.directory.accounts.lock().unwrap();
        match status {
            Some(status) => {
                if let Some(entry) = accounts.get_mut(email) {
                    entry.status = status;
                }
            }
            None => {
                accounts.remove(email);
            }
        }
    }

    pub fn directory_goes_down(&self) {
        self.directory
            .down
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn directory_comes_back(&self) {
        self.directory
            .down
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    pub async fn catalog(&self) -> Outcome<CatalogJson> {
        self.signed(self.api.get("/api/access/catalog"))
            .send()
            .await
    }
}

/// Every test's database starts as a copy of this: migrated, and the first release's default
/// grants applied, as `migrate` leaves it, with `ADMIN_ID` assigned `admin`, as a seed would.
pub const TEMPLATE: Template = Template::new("access", |pool| {
    Box::pin(async move {
        app_db::run_migrations(&pool, &[access::migrations()])
            .await
            .expect("migrate access");
        let ctx = Context::new(pool, Arc::new(app_domain::SystemClock));
        access::apply_default_grants(&ctx, Arc::new(workshop::modules(Release::First)))
            .await
            .expect("apply the defaults");
        access::assignments(&ctx)
            .ensure_assigned(&Principal::System, ADMIN_ID, ADMIN)
            .await
            .expect("assign the admin");
    })
});
