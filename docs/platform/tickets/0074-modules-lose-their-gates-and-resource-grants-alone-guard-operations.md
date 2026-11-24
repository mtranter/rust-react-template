---
id: 0074
name: Modules lose their gates, and resource grants alone guard operations
status: done
priority: P1
size: M
blocked_by: []
blocks: [0077]
source: ADR 0047 (accepted 2026-11-24), the first of three slices with 0077 and 0078
---

# Modules lose their gates, and resource grants alone guard operations

## Description

ADR 0047 removes every Module's gate (`module:<name>` / `use`), and no gate replaces it: an operation is allowed by its action's resource grant alone. This is the first slice. It changes only authorization and keeps every role's effective access exactly as it was. Capabilities on disk are 0077; showing Areas by permission is 0078.

**Load the `endpoint-auth` skill first.** Read ADR 0040 and 0047.

## Evidence (as of 2026-11-24)

- **`app_web`** (`crates/common/web/src/caller.rs`): `Gate` (with `const MODULE`), and `Caller<G>`, which admits only roles that `module:<G::MODULE>` / `use` allows.
- **`app_authz`** (`crates/common/authz/src/policies.rs`):
  - `Policies::gate`, `gate_kind` (`:334`) and `PoliciesBuilder::gate`, which takes a Module's gate policy;
  - `gate_entry` (`:682`), the gate's entry in the Catalog.
- **Each Module's `web/access.rs`** declares a gate struct and `pub type Caller = app_web::Caller<Gate>` (`crates/reference/notes/core/src/web/access.rs`).
- **Gate policy.** The only gate policy file is `crates/platform/access/core/policies/gate.yaml`, a built-in rule: `admin` always passes `module:access`. `access`' resources are `admin`'s anyway by built-in rules of their own (`subject.yaml` etc.).
- **Gate default grants:** `module:*` in the `default_grants.yaml` of `notes`, `users`, `note-search`, `assistant` and `module-template`.
- **Tests naming gates:** in `crates/platform/access/core/tests/access/`, the `workshop.rs` fixture and `granting.rs`, `catalog.rs` and `access.rs` in `behaviours/`; `crates/common/authz/src/tests/`.
- **The SPA treats gates specially:** `frontend/src/features/access/catalog.ts` (`resource.startsWith("module:")`), `frontend/src/test/access-fixtures.ts` and `role-permissions.test.tsx`.
- **`AccessService::apply_defaults`** (`crates/platform/access/core/src/application/service.rs:224`) is what `migrate` runs to apply default grants, once each.

## What to build

1. **`app_web`.** Delete `Gate`, and make `Caller` non-generic: a valid token's Principal, `401` without one, and no role check. Handlers take `app_web::Caller` directly. Delete each Module's gate struct and its `Caller` alias, `module-template`'s included.
2. **`app_authz`.** Delete `gate_kind`, `Policies::gate`, `PoliciesBuilder::gate` and `gate_entry`, so the Catalog has no gate entries. Loading refuses any policy, resource policy or default grant whose resource starts with `module:`, naming the file and line, as it does for other unknown resources.
3. **Policies.**
   - Delete `access`' `gate.yaml` and every `module:*` default grant.
   - Read each remaining default grant to `user` and confirm it is exactly what every signed-in user may do, with no gate to fence it any more. Note any that look too broad in this ticket's Resolution rather than changing them.
4. **Retiring the gate grants, once.** A step of `migrate` in `access`, run before `apply_defaults`, inside one unit of work.
   - **When it runs:** only while a marker says it hasn't run. The marker is a one-row table added by a new `access` migration, set by the step in the same unit of work. On a fresh database it finds no grants, does nothing and sets the marker. Without the marker, a later run, with no gate grants left, would revoke every resource grant.
   - **What it revokes:**
     1. Every grant on a resource of Module *m* held by a role that the gate of *m* didn't allow. A role was allowed if it held a grant on `module:<m>` / `use`, and for `access` also if it is `admin` (the deleted `gate.yaml`'s rule, hard-coded in the step with a comment saying where it came from).
     2. Then every grant on `module:*`.
   - Each revocation is `GrantedBy::System` and announced like an admin's.
   - **The rule set** is rebuilt once after the step.
5. **The SPA.** Delete the gate handling in `features/access/catalog.ts` and its uses. Fixtures and tests use resource grants (`workshop`'s `widget` resource, or whatever the fixtures name).
6. **Tests.**
   - `access`' tests and `app_authz`'s stop naming gates.
   - New behaviours in `access`' tests for the step:
     - a role granted a resource without its gate loses that grant;
     - a role granted both keeps the resource grant;
     - every `module:*` grant is gone, and each change is announced;
     - a second `migrate` changes nothing;
     - a fresh database changes nothing and applies the defaults as before.
   - `composed_app.rs` keeps passing unchanged: every guarded operation is still in its Module's `OPERATIONS`.
7. **Docs.**
   - `docs/platform/glossary.md`: delete **Gate**; **Catalog** lists resources only.
   - `crates/AGENTS.md`: the auth bullets and the reference table's "A guarded operation" row (no gate, `app_web::Caller`).
   - The `endpoint-auth` and `new-module` skills.
   - ADR 0040 itself stays as written; 0047 amends it.

## Acceptance criteria

- No `module:*` resource exists in code, policy or the grants table after `migrate`.
- A guarded operation is allowed exactly when the caller's roles hold its action's grant at a Scope that allows the row. A signed-in caller with no grants gets `403` (or `404` for a row they may not view), as before.
- After `migrate` on a database seeded before this change, every role can do exactly what it could before. A test compares each seeded role's allowed operations before and after.
- `make check-changed` passes, including e2e (the bootstrap admin still reaches Access).

## Notes

- Ticket 0077 moves the Modules into Capability folders, and 0078 adds `get_my_permissions` and Areas' `needs`. Don't start either here.
- 0075 and 0076 are descoped.

## Resolution

- **No gates.** `app_web::Caller` is a valid token's Principal and nothing else; `Gate`, every Module's gate struct and `Caller` alias, `Policies::gate`, `PoliciesBuilder::gate`, `gate_entry`, `GATE_ACTION`, `GrantRefusal::ScopeOnGate`, `RuleSet::plan_gate` and `Authorizer::allows_use_of` are gone. A resource, resource policy, label or default grant naming `module:<name>` fails loading at its file and line, saying Modules have no gates (ADR 0047).
- **Retiring the gates** is `AccessService::retire_gates` (`crates/platform/access/core/src/application/gates.rs`), run by `access::apply_default_grants` before the defaults while `access.gates_retired` (migration `0005`) has no row. One refinement to "a role was allowed if it held a grant on `module:<m>`": **every Principal holds `user`**, so when `user` held a gate, every role passed it, and none of that Module's grants is revoked. Without it, a role such as the workshop fixture's `maintainer` (granted `polish` with no gate of its own, but whose holders all passed the gate as `user`) would have lost what it could do. A grant on a resource no loaded Module has is left alone: it allowed nothing before and allows nothing now. What stays imprecise: a role whose grant took effect only beside another assigned role holding the gate (neither `user`) loses that grant; the holder loses nothing they couldn't regain by a grant to that role, and no one gains anything.
- **The rule set rebuilds once**: the step bumps `access.revision` once, in its one unit of work, so each instance rebuilds once at its next poll.
- **Default grants to `user`, read with no gate to fence them.** Each is what every signed-in user may do now. None changed. These look broad:
  - `users`: `user` / `list` and `view` at `any`: every signed-in user can list and read every user's profile, email included.
  - `note-search`: `noteSummary` / `search` at `any`: every signed-in user can search every note's summary, other authors' included, since the search has no `Columns` to scope it (it refuses only a `Where` plan).
  - `module-template` (`widgets`): `widget` / `*` at `any`: every signed-in user can create, view, rename, list and search every widget. Every module `make new-module` scaffolds starts so.
  - `notes` (create, and the rest at `author`) and `assistant` (`chat`, `start`, the rest at `owner`) are exact.
