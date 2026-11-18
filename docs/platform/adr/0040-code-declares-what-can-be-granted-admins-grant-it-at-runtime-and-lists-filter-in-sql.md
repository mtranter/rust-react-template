# Authorization: code declares what can be granted, admins grant it at runtime, and lists filter in SQL

Amended by [ADR 0041](0041-a-principals-roles-are-user-plus-its-assignments-in-access.md): a user's roles are `user` plus the roles `access` assigns its subject, held in the compiled rule set, not the token's `roles` claim.

Builds on [ADR 0025](0025-auth-is-one-jwt-bearer-scheme.md), which says who the caller is; this says what they may do, at three levels: a Module, an action (what an operation does), and a row. The app is single-tenant: there is one set of grants.

**Code declares what is possible; data says who gets it.** An admin must be able to give a role an action without a deploy, but what an action is, what a row's attributes are and which conditions can become SQL belong to the code. So:

| | Lives in | Changed by |
|---|---|---|
| Actions | a closed Rust enum per resource | developers |
| Scopes (named row conditions, e.g. `author`) | the Module's policy files | developers |
| Baseline rules: the `access` Module's own admin rules, and every DENY | the Module's policy files | developers |
| Default grants | the Module's policy files, applied once by `migrate` | developers |
| **Grants**: role → action, within a scope | the `access` Module's tables | **admins, at runtime** |

**A Principal is built on every guarded request.** The `JwtAuth` checker verifies the token, then hands its `Claims` to a `PrincipalResolver` (a port, in request data) for `Principal::User { id, roles, attr }`. The default resolver takes `id` from `sub` and `roles` from a `roles` claim (amended by ADR 0041: `user` plus the subject's assignments in `access`). `Principal::System` is the worker's and seeds' caller: it acts on facts already decided, so policies don't apply to it. Poem has no principal of its own; poem-openapi's `SecurityScheme` checker is where it is built, so the spec still shows the operation guarded and MCP's in-process calls get it too (ADR 0034).

**Policy files** are YAML in a Module core's `policies/`, embedded in the binary. Their format is inspired by Cerbos: resource policies and derived roles are, today, a strict subset of its syntax, under our own `apiVersion: auth.flagrante.io/v1`. Default grants are ours: Cerbos keeps grants in policies too, not in data.

```yaml
apiVersion: auth.flagrante.io/v1
derivedRoles:
  name: notes_scopes
  definitions:
    - name: author
      parentRoles: ["*"]
      condition:
        match:
          expr: R.attr.authorId == P.id
```

```yaml
apiVersion: auth.flagrante.io/v1
resourcePolicy:
  resource: note
  version: default
  importDerivedRoles: [notes_scopes]
  rules:
    - name: archived notes are read-only
      actions: [edit, archive]
      effect: EFFECT_DENY
      roles: ["*"]
      condition:
        match:
          expr: R.attr.archived == true
```

```yaml
apiVersion: auth.flagrante.io/v1
defaultGrants:
  - { role: user, resource: "module:notes", action: use }
  - { role: user, resource: note, actions: [create, list] }
  - { role: user, resource: note, actions: [view, edit, archive], scope: author }
  - { role: admin, resource: "module:notes", action: use }
  - { role: admin, resource: note, actions: ["*"] }
```

A default lists `action` or `actions` (`*` for every action of its resource), and expands to one grant per action when it loads.

- **Rules:** `actions` (`*` and `:`-segment wildcards), `effect` (`EFFECT_ALLOW`, `EFFECT_DENY`), `roles` (`*` for any), `derivedRoles`, `condition`, `name`.
- **Conditions** are a small expression language, a subset of CEL's syntax: `R.attr.<a>` compared with `==` or `!=` to `P.id`, `P.attr.<b>` or a literal (string, integer, boolean, `null`); `R.attr.<a> in [literals]`; combined with `match.all`, `any` and `none` (each `of:` a list). Nothing else: no functions, no arithmetic.
- **A scope** is a derived role whose `parentRoles` is `["*"]`: it means only its condition, so a grant can pair it with any role. A grant names `any` or one of its resource's scopes.
- **Combining:** anything not allowed is denied. Within one role, a matching DENY beats an ALLOW; across roles, an ALLOW from any role wins. A grant only allows: admins can't write a DENY.
- **Refused when loading, naming the file and line:** any other policy kind or field (principal and role policies, scopes, variables, constants, outputs, schemas), any other expression, a `version` other than `default`, and an `apiVersion` other than ours.

**The language is small so that every condition compiles to SQL.** A grant compiles to a rule (`roles: [role]`, `actions: [action]`, and its scope's condition), so the evaluator sees one rule set. It answers `plan(principal, action)` with `Allowed`, `Denied`, or `Where(filter)`: the conditions left once the principal's roles are known. The same `filter` is evaluated in memory against one loaded row, and pushed into a list's query as a `WHERE` clause by `app_db`, beside `push_list_clauses` (ADR 0018), so paging and totals count only rows the caller may see.

**The YAML is checked against Rust types when it loads, and a grant when it is made.** A resource implements `app_authz::Resource`: its `KIND`, a closed `Action` enum (like a sort key, no caller text reaches a check), and `attr(name)`; its DB Layer maps each attribute to a column or a module-authored SQL expression (`"archived" → "archived_at IS NOT NULL"`). Loading fails on an action, attribute or scope a policy names that doesn't exist, and on a Module's policy naming another Module's resource. Granting refuses an unknown resource, action or scope, so a grant in the tables always compiles. Roles come from the identity provider, so their names are not checked (amended by ticket 0031 and ADR 0041: roles are records in `access`, and a grant must name one).

**The rule set in force is rebuilt when grants change, and swapped whole.** Each process holds one compiled rule set: every Module's policy files plus every grant. The `access` Module keeps a revision, bumped in the same Unit of Work as each change. Every server instance reads it every 5 seconds and rebuilds when it has moved, so a change takes effect everywhere within that bound. A request never reads grants itself. A rebuild that fails keeps the rule set it had and logs at error.

**Defaults are applied once.** `migrate` applies each Module's default grants that it hasn't applied before, recording each one's key (Module, role, resource, action, scope) in the `access` schema. A revoked default stays revoked; one added in a later release is granted on that deploy. A Module doesn't write the `access` schema: `migrate` hands each Module's defaults to `access` through its Module API.

**Admins can't lock themselves out.** The `access` Module's own gate and its admin actions are granted to `admin` by baseline rules, never by grants, so no revocation can remove them.

**Three levels, one engine:**

- **Module.** Each Module's gate is the resource `module:<name>` and the action `use`, granted on roles only (a scope there is refused). Its operations take `app_web::Caller<G>` instead of `JwtAuth`, where `G` names the Module; the extractor yields the `Principal` only after the gate allows it. No token is `401`; a token whose roles the gate doesn't allow is `403`. A Module's handlers can't reach a Principal any other way.
- **Action.** Each operation declares the action it performs. The service plans that action before loading anything; `Denied` is `403`.
- **Row.** A `Where` plan is checked against the row once it is loaded. Refused, it is `404` when the principal may not `view` that row, so its existence isn't revealed, and `403` when they may view it but not do this. A list applies the plan to its query.

**The catalog is what admins grant from.** `access` serves every Module's resources, their actions with the operations performing each, and their scopes, from the loaded policies. An action nothing allows is listed as unreachable.

**Secure by default.** A composed test fails on any operation without a security requirement unless it is in `PUBLIC` (`crates/server/src/`), with its reason, as `REST_ONLY` lists operations kept off MCP.

## Considered options

- **Gatehouse** (`gatehouse` crate): policies as typed Rust closures, vetoes for deny, a trace on every decision, much less code to own. But a list is filtered in memory after the query, or paged through a source written by hand, so nothing reaches SQL, and paging and totals break. Its policies are code, so no grant could change without a deploy. At 0.6 it had just broken its API, with one maintainer.
- **Cerbos itself**, as a sidecar PDP with its Admin API and a database store. Full CEL, policies editable at runtime, and a query plan API. But it's a second process to deploy and reach on every request (against ADR 0005's one artifact), admins would edit whole policies, conditions and all, and its query plans are an AST each project would still compile to SQL itself.
- **Policies only in files.** Simplest, but every change of who may do what is a deploy.
- **Whole policies in the database, edited by admins.** As flexible as files at runtime, but admins would write conditions, which could fail to compile to SQL or name attributes that don't exist, discovered only when the rule set is rebuilt.
- **Exactly Cerbos's `apiVersion`.** Claims a compatibility we don't promise and may drift from. Keeping the rest of the syntax identical means moving to Cerbos later is a one-line change per file, while the subset holds.
- **Full CEL** (a CEL interpreter crate). Expressive, but most expressions can't become SQL, so lists would fall back to filtering in memory.
- **Postgres row-level security.** The database would enforce the row level, but the principal would ride in a session setting per transaction, the rules would live in migrations apart from the module and action levels, and a module lifted into a service on another store would lose them.
- **Pushing grant changes over the bus** (ADR 0039). Instant, but it makes Valkey required in every app, and the bus is best effort, so a poll would still be needed for a gap. A 5-second bound is enough for a change of permissions.
- **Default grants as Seeds.** Seeds don't run in production builds (ADR 0028).
- **Default grants in YAML rules**, not in data. Admins could never revoke them.

## Consequences

- A use case that changes or reads data takes `&Principal`; reactions and seeds pass `Principal::System`.
- A Module adds an action and, usually, a default grant for it; `make new-module` scaffolds a gate, a scope-free resource policy and default grants for the template's root.
- A rule the subset can't express is a change to the subset, made here first, not an escape hatch per Module. A new scope is a code change and a deploy, after which admins can grant within it.
- A revoked grant still allows for up to 5 seconds on each instance.
- MCP lists every tool whatever the caller's roles; a call the policies refuse is a `403` or `404` result.
- Roles come only from the token. Groups, and roles the app assigns to users itself, are out of scope: either would be a different `PrincipalResolver`. (Amended by ADR 0041: roles come only from `access`' assignments, and the token's are ignored.)
- The format is ours to keep: a feature Cerbos adds later isn't ours until this ADR is amended.
