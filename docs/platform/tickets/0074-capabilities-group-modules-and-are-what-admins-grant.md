---
id: 0074
name: Capabilities own Modules in one folder each, gates go, and each App shows the Areas its user may use
status: needs-triage
priority: P1
size: L
blocked_by: []
blocks: []
source: conversation on 2026-11-24, following ADR 0046's shell; decision drafted as ADR 0047 (option B)
---

# Capabilities own Modules in one folder each, gates go, and each App shows the Areas its user may use

## Description

ADR 0047 (proposed):
- A Capability is one product of the suite. It owns a folder of Modules in `crates/capabilities/<capability>/`, and has one App in the SPA.
- Modules have no gate any more: resource grants alone guard operations.
- Each Area of an App declares the action it needs. A user is shown the Areas they may use, and an App when they may use any of its Areas.

## Evidence (as of 2026-11-24)

- **The gate is per Module and checked on every operation.** `app_web::Gate` has `const MODULE`, and `Caller<G>` admits only roles that `module:<G::MODULE>` / `use` allows (`crates/common/web/src/caller.rs`). `app_authz`'s `gate_kind` formats it (`crates/common/authz/src/policies.rs:334`), and `PoliciesBuilder::gate` takes a Module's `gate.yaml`.
- **Gate grants sit in each Module's default grants**, e.g. `crates/reference/notes/core/policies/default_grants.yaml` (`module:notes`). `crates/platform/access/core/policies/gate.yaml` keeps admins able to reach `access`. `access`' own resources are `admin`'s by its policies anyway (`domain/access.rs`).
- **The Catalog lists Module gates** (`crates/platform/access/core/src/web/handlers.rs:136`, `models.rs:19,100`). The SPA groups by `moduleLabel` (`frontend/src/features/access/catalog.ts`). The tests and fixtures use `module:workshop` and `module:access`: `workshop.rs`, `granting.rs`, `catalog.rs` and `access.rs` in access' tests, and `access-fixtures.ts` and `role-permissions.test.tsx` in the SPA.
- **The SPA hides Apps only case by case.** `useShellApps` always shows Notes, and shows Access via `useManagesAccess`, which succeeds only for those the Catalog answers (`frontend/src/shell/apps.ts:58`).
- **Resource kinds today:** `note`, `noteSummary`, `user`, `assistant`, `conversation`, `grant`, `role`, `assignment`, `subject`.
- **Paths.** `crates/modules/` doesn't exist yet. The workspace globs `crates/*/*/core` (`Cargo.toml`), and a glob matching nothing fails. These scripts name `crates/modules/`: `new-module.sh`, `check-modules.sh`, `check-changed.sh` and `check-sql-schemas.sh`.
- **The glossary** lets one Module serve several Apps (The UI), defines Gate (Authorization), and says "capability trait" for a Module API's ports.

## Proposed approach

Three slices, each passing `make check-changed` on its own. Load `endpoint-auth` before slices 1 and 2.

1. **No gates; Capabilities on disk** (backend).
   - Remove `Gate` and the gate check from `Caller` in `app_web`. A Module's `Caller` becomes a plain alias, or `app_web::Caller` directly.
   - Remove the gate from `app_authz`: `gate_kind`, `PoliciesBuilder::gate`, and the Catalog's gate entry. Loading refuses a policy naming `module:*`.
   - Delete each Module's `gate.yaml` and its `module:*` default grants. Check that every remaining default grant to `user` is exactly what every user may do.
   - Add `capability.yaml` (`id`, `label`) for `crates/reference/` (`notes`), `access` and `assistant`. `app_authz` takes each Module's Capability, for the Catalog.
   - `Cargo.toml` and the scripts `new-module.sh` (takes `CAPABILITY=`; creates the folder and yaml; adds the workspace globs when it makes the first Capability), `check-modules.sh`, `check-changed.sh` and `check-sql-schemas.sh`: `crates/capabilities/*/*`. Update `module-template`.
   - `init.sh`: no `crates/capabilities/` and no globs for it. `check-init` runs `make new-module CAPABILITY=crm NAME=contacts` in the scratch app.
   - **The `migrate` step.** It revokes, per role, every grant on a Module's resources that the role held without that Module's gate, then revokes every `module:*` grant. Each change is announced. Test it with:
     - a role granted a resource but not its gate, which loses the grant;
     - a role granted both, which keeps the resource grant.

2. **Granting by Capability** (`access`).
   - The Catalog groups each Capability, then its Modules' resources (`capability`, `capabilityLabel`). `make generate`.
   - `get_my_permissions`: the caller's allowed resource and action pairs, at any Scope, from the compiled rules. Open to any signed-in caller (open question 1). It goes in `REST_ONLY` and in `check-init`'s `platform_ops` if it stays off MCP.
   - Tests: replace `module:workshop` with resource grants. Test that `get_my_permissions` answers a role's grants and nothing denied.

3. **Areas declare what they need** (frontend).
   - Rename `shell/apps.ts` to `shell/capabilities.ts`: `ShellApp` becomes `Capability`, `useShellApps` becomes `useCapabilities`, and each entry is keyed by its Capability's id.
   - Each `Area` gains `needs: { resource, action }`. `useCapabilities` keeps the Areas whose need `get_my_permissions` grants, and the Apps with any left. Areas today:
     - notes: `note` / `list`, and search, `noteSummary` / `list` (check the action name)
     - access: Users `subject` / `list`, Roles `role` / `list`, Permissions `grant` / `list`
   - Delete `useManagesAccess`. The launcher, switcher, breadcrumb and Go to already read the one list.
   - Access screens group by Capability. Update fixtures, `role-permissions.test` and `shell.test`, with a test that a user without `subject` / `list` sees Access with Roles but no Users.
   - `init.sh`'s generated `apps.ts` becomes `capabilities.ts`. Update the screens, e2e (the bootstrap admin still reaches Access) and `check-init`.
   - `check-modules` (or a frontend test against `openapi.json`'s catalog): every Area's need names a resource and action a Module declares.

Docs, in the slice that makes each one true:
- ADR 0047 to accepted.
- Glossary: Capability; App and Area as in the ADR; Gate removed; "Module API trait".
- `crates/AGENTS.md`: paths, guarding, "Adding a module".
- `frontend/AGENTS.md`: Shell.
- The `endpoint-auth` and `new-module` skills.

## Acceptance criteria

- No `module:*` or `capability:*` resource exists. Every guarded operation is allowed by its resource grant alone.
- After `migrate`, no role can do anything it couldn't before, and none has lost anything it could do.
- A user is shown exactly the Areas whose required action they hold, and the Apps with any such Area. A non-admin sees no Access. A role with only `role` / `list` sees Access with Roles alone.
- The Catalog groups resources by Capability.
- `make check-modules` fails on a Module whose folder's Capability has no registry entry (apart from the platform's without an App), and on an Area naming an unknown action.
- `make new-module CAPABILITY=crm NAME=contacts` in a fresh `make init` app scaffolds `crates/capabilities/crm/contacts/`, whose tests pass. `make check-init` passes.
- `make check-changed` passes, including e2e.

## Decisions (2026-11-24)

1. **Any signed-in caller may call `get_my_permissions`.** It answers only what the caller may do. It is a third kind of operation, neither `PUBLIC` nor guarded by a resource grant, and is listed with its reason beside `PUBLIC` (`crates/server/src/public.rs`). A resource grant to `user` was rejected because an admin could revoke it from a role, and every App would vanish for that role's users.
2. **An Area needs exactly one action.** "Any of" or "all of" is added when an Area first needs it.

## Notes

- Tickets 0075 (disabling a Capability) and 0076 (embedded panels) are descoped for now (`wontfix`), so this ticket blocks nothing.
