---
name: endpoint-auth
description: How the backend authenticates and authorizes requests, and how to guard an operation, restrict one to admins or another role, scope rows to their owner, add a role, assign roles in seeds and tests, or make an operation public. Use when adding or changing any operation, its access rules or policies, or when explaining or debugging a 401, 403 or unexpected 404.
---

# Endpoint auth (ADR 0025, 0040, 0041)

Authentication is one JWT bearer scheme; authorization is policies declared in code and grants held as data, of actions on resources (engine `app-authz`). A Module has no gate (ADR 0047): an operation is allowed by its action's grant alone. **Never hand-roll a role check** in a module: express it as an action and a grant.

Extends `crates/AGENTS.md`; if this file and the code disagree, the code wins: fix this file in the same change.

| Doing | Copy |
|---|---|
| A guarded operation and its policies | `crates/reference/notes`' `policies/`, `web/access.rs` (`OPERATIONS`), a handler's `app_web::Caller`, `domain/access.rs` (`impl Resource for Note`), `di.rs` (`policies()`), and a use case's `self.authorize(..)` |
| A row only some may act on, and a list holding only the rows the caller may see | `crates/reference/notes`' `db/repository.rs` (`NoteColumns`, `push_scope` in `list`), its service's `load_for(..)` and `list(..)`, and its tests' `behaviours/scoping.rs` (the agreement test) |
| Who may do what at runtime: roles, grants, assignments, defaults, the rule set's rebuild | `crates/platform/access` (its `application/rules.rs`, `application/roles.rs`, `application/assignments.rs`, `apply_defaults`), wired in `server::rules` / `server::apply_default_grants`, journeys in `crates/reference/journeys/tests/journeys/granting.rs` and `assigning.rs` |

## Authentication

One JWT Bearer scheme (`app_web::JwtAuth`, `crates/common/web/src/auth.rs`), whose checker resolves the token's claims to a `Principal` through an `Arc<dyn PrincipalResolver>` in request data, or by default the `Authorizer` there: `sub` is the id, the roles are `user` plus those `access` assigns the subject, read from the rule set in force, so no request reads the database for them. No or bad token: **401**.

- The server's resolver is `access::sign_ins` (put in request data by `access::module` through `Module::providing`), which also records the subject as seen (with the token's `name` and `email` claims), writing nothing for a subject recorded within the hour.
- **The Actor** (`Principal::User { actor: Option<Actor>, .. }`, `principal.actor()`) is the agent acting for the subject, set by `Claims::actor` in the default resolver: the outermost `act.sub` with the nested `act`s as its `chain`, else the token's `client_id` (or `azp`) when it isn't in `APP_FIRST_PARTY_CLIENTS` (`Authorizer::with_first_party_clients`; default `APP_OIDC_CLIENT_ID`), else `None`. A client-credentials agent's subject is its own client, so its actor is that client: `access` records it as a Subject and roles are assigned to it like a user's. `Principal::user(..)` has no actor; `.acting(Some(Actor::new("agent").behind([..])))` gives it one in a test.
- A token's `roles` claim is ignored (`Claims` has none). `APP_BOOTSTRAP_ADMINS` (comma-separated subjects, `Authorizer::with_bootstrap_admins`) always hold `admin`.
- Don't build a user store or token issuance unless asked. Configuring the provider is the `configure-auth-provider` skill.
- In dev the Vite proxy signs every `/api` request as the seeded Alice (`frontend/vite.config.ts`), whom the seed of `crates/reference/users` assigns `admin`.
- Tests issue tokens with `JwtVerifier::issue(subject, ttl)`, or, carrying a profile or an agent, `JwtVerifier::sign(&Claims::lasting(subject, ttl)?.named(..).with_email(..))`, `.acted_by(["outer", "inner"])` (nested `act`), `.from_client(..)` (`client_id`) or `.authorized_party(..)` (`azp`); `access`' driver's `sign_in_as_agent(client)` signs in as a client-credentials agent.

## Authorization: what every operation needs

Every operation has a security requirement unless it is in `PUBLIC` (`crates/server/src/exempt.rs`) with its reason; `composed_app.rs` fails on one with none that isn't listed, and on a listed one that has one. Beside it, `SIGNED_IN` lists, with their reasons, the operations any signed-in caller may call: they take `Caller` but perform no action and are in no `OPERATIONS` (today `get_my_permissions`). `composed_app.rs` fails on an operation in both lists, and on a `SIGNED_IN` one with no security requirement or an `OPERATIONS` row. So for every other operation:

1. **Its handler takes `app_web::Caller`** (`use app_web::Caller;`), never `JwtAuth`. It yields a valid token's `Principal` (no or bad token is **401**) and the token (`caller.token()`, for calls made as the caller), and checks no role: the use case's action does.
2. **Its use case takes `&Principal`** first and plans its action before it loads or begins anything (`self.authorizer.authorize::<Note>(principal, NoteAction::Edit)?`: `Denied` is **403**). The service holds `ctx.authorizer()`. Reactions, deferred jobs and schedules pass `Principal::System`, which policies don't apply to.
3. **A row is checked once loaded, before any domain action** (`self.authorizer.check(principal, &plan, &note)?`, in the `load_for` of `crates/reference/notes`): refused, it is **404** when the principal may not `view` that row either (the resource's `Resource::VIEW`), so its existence isn't revealed, and **403** when they may. So an author's archived note is 403 to edit (the policy's deny rule answers before the domain's 409, which stays for `Principal::System`), another user's 404. Say both in the operation's summary.
4. **A list asks for its rows** (`self.authorizer.rows::<Note>(principal, NoteAction::List)?`, `Denied` is **403**) and hands them to its repository, which pushes them into its SQL with `app_db::push_scope::<XColumns>`. `XColumns` (`impl app_db::Columns` in `db/`) maps each of the resource's `ATTRS` to a column or a module-authored expression, with its kind, so a policy's value is bound as the column's type and never written into the SQL. Its DB tests check it with `XColumns::assert_every_attribute_is_mapped()` and `app_authz::testing::assert_lists_agree` (the `testing` feature): for several principals and every action, the scoped query holds exactly the rows `Filter::matches` does in memory, so the SQL and the row check can't drift. Rerun it after adding an attribute, scope or rule. Never filter rows after the query.
   - A read module's or a projection's search has no `Columns` yet: it refuses any plan but `Allowed` (`crates/reference/note-search`, the template's `search`) rather than show rows the caller may not see.
5. **Its action exists and is granted by default**: the resource's closed `Action` enum (`impl app_authz::Resource` in `domain/access.rs`, beside the aggregate; a read module's on its read model) has it, and the module's default grants, its `policies/default_grants.yaml`, grant it, usually to `user` and `admin`. A module's `policies()` in `di.rs` builds its `Policies` from `policies/` with `app_authz::policy_file!`, registered with `Module::with_policies(policies)`; `make check-modules` fails on a module with operations and none. The server, worker and `migrate` refuse to start on policies that don't load, naming the module, file and line.
6. **Its row in `OPERATIONS`** (`web/access.rs`, registered with `.performing(OPERATIONS)`): the operation id, resource and action, what the access catalog shows an admin. `composed_app.rs` fails on a guarded operation missing from it, or naming an action no policy has.

## Policies (`policies/`, Cerbos-style YAML)

- `default_grants.yaml`: `- { role: user, resource: note, action: create }`, `actions: [..]`, optionally `scope: <derived role>`.
- `derived_roles.yaml`: scopes such as `author`, `condition: { match: { expr: R.attr.authorId == P.id } }`.
- `<resource>.yaml`: a resource policy's rules, e.g. an `EFFECT_DENY` on edits while `R.attr.archived == true`.
- `labels.yaml`: what admins see at `/access`, in the catalog's `label`s (the catalog groups resources by Capability, labelled by the `capability.yaml` above the module, ADR 0047): the module's name (`module: Notes`), and per resource its `label` and its `actions` and `scopes` (`any` included) as `name: words`. Loaded with `.labels(policy_file!("policies/labels.yaml"))`; naming a resource, action or scope the module lacks fails loading. Anything left out shows as its name in words (`items:add` → `Items add`), so label a new action when you add it.

Copy the files of `crates/reference/notes/core/policies/`.

## Grants, roles and assignments are data

The platform's `access` module (`crates/platform/access`) holds them.

- **Default grants apply once.** `migrate` applies each default once: editing a default already applied changes nothing deployed, a new one is granted by the next `migrate`, and one an admin revoked stays revoked. Removing an action leaves its stored grants stale (listed, skipped) until an admin revokes them. So a role restriction in `default_grants.yaml` is a default, not a guarantee. A resource named `module:<name>` (a gate's, before ADR 0047) is refused when policies load, naming the file and line.
- **Gates were retired once.** Before it applies the defaults, `migrate` runs `access`' `retire_gates` (`application/gates.rs`) while `access.gates_retired` has no row: it revokes, as `System` and announced, every grant on a Module's resources whose role its gate didn't let through (no grant of `module:<m>` / `use` to that role or to `user`, and for `access` not `admin`), then every `module:*` grant, and sets the row in the same unit of work, so no role gains or loses anything. Never clear that row: run again with no gate grants left, it would revoke every grant.
- **Grants**: admins grant and revoke over `list_grants`, `grant_access`, `revoke_grant` and `get_access_catalog` (and the SPA's `/access`), refused `400` naming the field when the role doesn't exist or the catalog lacks the rest, and `409` when equal to a grant made. `grant_access_in_bulk` (`POST /api/access/grants/bulk`, the same `grant`/`create` action) grants a role every action of `actions` on every resource of `resources` that the resource has, within every scope of `scopes` (`["any"]` unless given) it offers, in one unit of work with one revision bump and one event per grant: grants equal to ones made are `skipped`, not `409`, and it is refused `400` naming `role`, `resources`, `actions` or `scopes` when one is unknown or in no resource given, nothing would be granted, or more than 500 grants would be.
- **Roles** are records (`list_roles`, `create_role`, `delete_role`): a grant's role must name one (a foreign key in `access`' own schema), `admin` and `user` are built in, a role a grant names can't be deleted, and `migrate` creates a role a default grant names when it first applies that grant. A test granting a role of its own creates it first (`create_role`).
- **Assignments**: who holds a role beyond `user` (`assign_role`, `unassign_role` with `If-Match`; `list_subjects` lists everyone signed in, assigned a role or a bootstrap admin, with their roles): `user` is refused `400`, a role that doesn't exist `400` naming `role`, deleting a role an assignment names `409`, and unassigning a bootstrap admin's `admin`, or the last stored `admin` while `APP_BOOTSTRAP_ADMINS` is empty, `409`.
- **Invitations** (ADR 0048): admins invite people by email (`invite_user`, `list_invitations`, `resend_invitation`, `revoke_invitation`, the `invitation` resource, `admin`'s by baseline rules, and the SPA's `/access/invitations`): `access` has the Identity Directory (`IdentityDirectory`, Cognito's adapter in `src/cognito/`, configured by `COGNITO_USER_POOL_ID`, and without it `501`) create them, assigns their roles to the `sub` it answers with, and accepts the Invitation at that `sub`'s first sign-in; `access`' tests fake the directory in their driver, and `fake-oidc` serves a stand-in for Cognito's API at `/cognito` for the adapter's test, the journeys and `make e2e`.
- **Seeds and journeys**: a seed assigns its accounts' roles through `access_api::RoleAssignments` as `System` (the `module(access::assignments)` of `crates/reference/users`; `Module::with_seed` is given the `Context`), a journey through the admin's API or that Module API, never in a token.
- **The rule set**: each process starts from every module's baseline rules (`server::authorizer`); a server instance then reads the stored grants and assignments before it serves (`server::rules`, `Rules::refresh`) and every `access::POLL_EVERY` (5 s) after, rebuilding its rule set, assignments included, when `access.revision` has moved, keeping the old one on a failed read. The worker doesn't poll: it acts as `System`. `access`' own actions are `admin`'s by baseline rules, so no revocation locks admins out.
- **A module's own tests have no `access`**: their driver compiles its defaults in memory (`Authorizer::from_policies`) and gives a signed-in user roles with `Authorizer::assign_in_memory`.

## Recipes

### Restrict an operation to admins (or any one role)

1. Add the action to the resource's `Action` enum (`#[strum(serialize = "purge")]` where the wire name differs).
2. In `default_grants.yaml` grant it only to that role, `- { role: admin, resource: note, action: purge }`, and check no wider grant to another role (`actions: ["*"]`) covers it.
3. In the use case, first thing: `self.authorizer.authorize::<Note>(principal, NoteAction::Purge)?;`
4. Add `Performs::new("purge_notes", "note", "purge")` to `OPERATIONS`.
5. The handler takes `caller: Caller`. Say who may call it, and its 403/404s, in the operation's summary: it's also the MCP tool's description.
6. Test: sign in with the role through the driver (which uses `Authorizer::assign_in_memory`) and assert it succeeds, and that a plain `user` is refused and nothing changed or was announced.

### Restrict a whole module to a role

There is no gate to grant (ADR 0047): grant each of its resources' actions only to that role in `default_grants.yaml`, and none to `user`. Everyone else gets 403 on every operation. A default grant to `user` is exactly what every signed-in user may do, with nothing outside it to fence the module, so ask "should every user be able to do this?" of each one.

### Scope rows to their owner

Define a derived role in `derived_roles.yaml` and grant the actions with `scope: <it>`. Add each attribute the condition reads to the resource's `ATTRS` and map it in `XColumns`; rerun the agreement test.

### A new role

Name it in a default grant and `migrate` creates it; or an admin creates it (`create_role`) and assigns it (`assign_role`) at `/access`.

### A public operation

Leave its security requirement off and list it in `PUBLIC` (`crates/server/src/exempt.rs`) with the reason. Keep this rare.

### An operation any signed-in caller may call

Its handler takes `Caller` and its use case calls no `authorize`; list it in `SIGNED_IN` (`crates/server/src/exempt.rs`) with the reason, and leave it out of `OPERATIONS`. Only for what no admin should be able to take away, as `get_my_permissions` answers only what the caller may do. Keep this rarer still.

### What the caller may do, and the SPA's Areas

`get_my_permissions` (`access`, `AccessService::permissions`) answers every resource and action in the loaded policies whose plan for the caller, in the rule set in force, isn't `Denied`: the plan the server enforces, so built-in rules (`admin` on `access`' resources) count as grants do, and a scoped grant counts. The SPA's shell shows each Area whose `needs` it holds (`frontend/AGENTS.md`); hiding is no protection, so every operation still plans its own action. `make check-modules` fails on an Area naming an action no registered Module declares (`dump-spec --actions` lists them).

## Debugging

- **401**: token missing, expired, or failing `iss`/`aud`/signature; see the `configure-auth-provider` skill.
- **403 everywhere in one module**: the caller's roles hold none of its resources' actions.
- **403 on one operation**: no grant for that action, or a resource rule denies it (e.g. archived).
- **404 on a row that exists**: the caller may not `view` it (its scope didn't match).
- **A just-assigned role doesn't apply**: allow up to 5 s for the rule set to refresh.
