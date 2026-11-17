---
id: 0029
name: Admins grant and revoke roles' actions at runtime, taking effect on every instance within 5 seconds, without a deploy
status: ready-for-agent
priority: P1
size: L
blocked_by: [0026, 0027]
blocks: []
source: conversation, 2026-11-17 (system admins associate roles with actions without waiting for a redeploy; single-tenant; groups out of scope; defaults applied as data)
---

# Admins grant and revoke roles' actions at runtime, taking effect on every instance within 5 seconds, without a deploy

## Description

After ticket 0027, who may do what is fixed at startup from each Module's default grants: changing it is a deploy. ADR 0040 moves grants into data owned by a platform Module, `access`, which:

- stores grants, applies Modules' defaults once, and serves the catalog;
- lets admins grant and revoke, over REST, MCP and a screen;
- bumps a revision that every server instance polls, rebuilding its rule set when it moves.

Single-tenant: one set of grants. Roles come from the token; groups are out of scope.

## Evidence (as of 2026-11-17)

- After ticket 0027, the `RuleSet` is compiled once at startup from policy files and default grants (`Authorizer`, `app-authz`), and is never replaced.
- Seeds don't run in production builds (ADR 0028), so they can't carry default grants.
- Each module migrates its own schema (ADR 0021). There is no hook after every Module's migrations, which is where defaults must be applied.
- Ticket 0025's acceptance criteria say an app straight out of `make init` has no operations and `/mcp` lists no tools. `access` is platform, not a reference Module, so it stays registered and adds both (decided 2026-11-17).
- The scripts know two folders of Modules: `crates/modules/*` (registered) and `crates/reference/*` (compiled, not registered). See `scripts/check-modules.sh`, `check-changed.sh`, `check-sql-schemas.sh` and `new-module.sh`.

## Proposed approach

1. **The Module.** `access` lives in `crates/platform/access/{api,core}`. It is registered first in `modules()`, built as `access::module(policies)` from every other Module's `Policies`, as `notes::module(users::directory)` is built.
   - **Platform, not the app's.** `crates/platform/` sits beside `crates/common/` and `docs/platform/` as the template's, so an app pulling later platform changes (`TEMPLATE_VERSION`) knows which Modules are its own (`crates/modules/`) and which aren't.
   - **The scripts learn the folder:** `check-modules.sh`, `check-module-deps.sh`, `check-sql-schemas.sh`, `check-changed.sh`, `new-module.sh` (a name `access` is taken). The workspace's member globs (`crates/*/*/core`, `crates/*/*/api`) already cover it. A Module there is registered with the server, as `crates/modules/*` is.
   - **`make init` keeps it registered** (ticket 0025, done first):
     - `init.sh` leaves `crates/platform/` alone;
     - its "no operations, no tools" criteria and `check-init.sh` become "only `access`'s";
     - the app's ticket 0001 doesn't retire it.
2. **Its own policies.** Baseline rules, not grants (ADR 0040): `admin` may `use` `module:access` and do every `grant` action. Nothing it stores can remove them.
3. **Domain.**
   - `Grant` is an Aggregate root: id, role, resource, action, scope, `grantedBy`, `grantedAt`.
   - Its topic carries `Granted` and `Revoked`, with the grant's state (ADR 0012). That is the audit trail.
   - A grant equal to an existing one is refused as `409 conflict`.
4. **Schema** `access`:
   - `access.grant`;
   - `access.revision`: one row, bumped in each grant's Unit of Work;
   - `access.applied_default`: the key of each default applied.
5. **Operations**, each with an `operation_id` and a summary written for an agent (ADR 0034):
   - `list_grants`: filter by role, resource or action; stale grants (ADR 0040) are flagged.
   - `grant_access`: role, resource, action, scope. It is checked against `Policies::check` and refused `400` with the field at fault.
   - `revoke_grant`: by id, with `If-Match`.
   - `get_access_catalog`: resources, actions with the operations performing each (`OPERATIONS`, ticket 0027), scopes, and unreachable actions.
6. **Defaults.** `migrate`, after every Module's migrations, hands every Module's default grants to `access`. In one Unit of Work, `access`:
   - grants each one whose key isn't in `applied_default`, as `System`;
   - records the key;
   - bumps the revision.

   A revoked default isn't granted again. A default later removed from its file is left as it is.
7. **Rebuilding.**
   - The `Authorizer` moves into `Context` (`ctx.authorizer()`), so every Module's services share one.
   - Each server instance runs a task that reads `access.revision` every 5 seconds and, when it has moved, reads every grant, compiles a `RuleSet` and swaps it in.
   - At startup the server waits for the first compile before serving.
   - A failed read or compile keeps the rule set it had and logs at error. Stale grants are logged at warn, once per revision.
   - The worker doesn't poll: it acts as `System`.
8. **Screen.** A frontend feature, `features/access`, reached by `admin` only:
   - a table of grants, sortable and filterable by role, resource and action, with stale ones marked;
   - a grant dialog whose resource, action and scope choices come from the catalog, and whose role is free text;
   - revoke with confirmation, handling `412`.
9. **Docs.**
   - `crates/AGENTS.md` and `frontend/AGENTS.md`: `access` is platform; a Module's defaults are its `default_grants.yaml`.
   - Glossary: **Revision**, **Catalog**, **Default Grant**.

## Acceptance criteria

- [ ] On a fresh database, `migrate` grants every Module's defaults. Run again, it grants nothing.
- [ ] A default revoked by an admin isn't granted again by the next `migrate`. A default added in a later release is granted by that release's `migrate`.
- [ ] Journey:
  - [ ] `support` is refused `note:view` (`403`);
  - [ ] an admin grants it with scope `any`;
  - [ ] within 5 seconds (driven by the test's clock or the poll interval, not a sleep) `support` can view any note;
  - [ ] after revoking, `support` is refused again.
- [ ] Journey, two server instances on one database: a grant made through one takes effect on the other within the bound.
- [ ] A grant naming an unknown action or scope is refused `400` with the field. A duplicate is refused `409`. Neither bumps the revision.
- [ ] No grant or revocation removes `admin`'s access to `access`.
- [ ] A grant whose action a later release removed is listed as stale and skipped, and the server still starts.
- [ ] The catalog lists `notes`' actions with their operations and the `author` scope.
- [ ] The screen grants and revokes against MSW in `src/test/`, and is hidden from a user without `admin`.
- [ ] After `make init` (`make check-init`), `access` is registered: its operations are the only ones in the spec, and its tools the only ones on `/mcp`.
- [ ] `make generate` is committed, and `make check` passes.

## Open questions

- **How the admin screen is reached.** Whether the frontend learns the caller's roles from the token or from an `access` operation (`get_my_access`) answering what they may do.
