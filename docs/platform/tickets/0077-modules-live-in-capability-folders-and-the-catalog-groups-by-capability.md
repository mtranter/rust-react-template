---
id: 0077
name: Modules live in Capability folders, and admins grant from a Catalog grouped by Capability
status: ready-for-agent
priority: P1
size: M
blocked_by: [0074]
blocks: [0078]
source: ADR 0047 (accepted 2026-11-24), the second of three slices with 0074 and 0078
---

# Modules live in Capability folders, and admins grant from a Catalog grouped by Capability

## Description

ADR 0047: a Capability is one product of the suite and owns a folder of Modules, each Module in exactly one. This slice:
- puts that on disk;
- teaches the scripts, `make new-module` and `make init` about it;
- groups the Catalog, and the access screens, by Capability.

It changes no permissions.

**Load the `new-module` skill first.** Read ADR 0047.

## Evidence (as of 2026-11-24)

- **There's no `crates/modules/` yet.** App Modules would go there today. The platform's are in `crates/platform/<module>/` (`access`, `assistant`), and the reference Modules in `crates/reference/` (`notes`, `users`, `note-search`, plus `journeys`).
- **The workspace members** are `crates/*/*/core` and `crates/*/*/api` (`Cargo.toml`), and Cargo fails on a member glob matching nothing.
- **Scripts naming `crates/modules/`:** `scripts/new-module.sh` (`:29,44,47,55`), `check-modules.sh` (`:56,60`), `check-changed.sh` (`:37,62`) and `check-sql-schemas.sh` (`:16`).
- **The Catalog groups by Module:** `app_authz::Catalog` has `module` and `label` (`crates/common/authz/src/policies.rs`, `pub struct Catalog`), served by `get_access_catalog` (`crates/platform/access/core/src/web/handlers.rs:136`, `models.rs`). The SPA groups by `moduleLabel` (`frontend/src/features/access/catalog.ts`).
- **The shell's registry is `frontend/src/shell/apps.ts`,** whose entries are keyed by `path`.

## What to build

1. **`capability.yaml`**, `{ id, label }`, where `label` is the admin-facing name and `id` matches `^[a-z][a-z0-9-]*$`:
   - one in `crates/reference/`, `{ id: notes, label: Notes }`;
   - one in each of `crates/platform/access/`, `{ id: access, label: Access }`, and `crates/platform/assistant/`, `{ id: assistant, label: Assistant }`.

   App Modules go in `crates/capabilities/<capability>/<module>/{core,api}`, beside their Capability's `capability.yaml`.
2. **Each Module knows its Capability.** The server reads the `capability.yaml` above each Module's core (embedded at build time, e.g. by a `build.rs` in `server` or `include_str!` from each Module's `di.rs`), and `app_authz::Policies` carries the Capability's id and label.
3. **The Catalog groups by Capability.** `get_access_catalog` answers a list of Capabilities, each with `capability`, `label` and its Modules' resources (each resource keeps its Module's label too). Then:
   - `make generate`;
   - the access screens (role page, permissions page) group by Capability, then resource;
   - update `catalog.ts`, the fixtures and the tests.
4. **Workspace and scripts.**
   - `Cargo.toml` keeps its globs. `crates/capabilities/*/*/core` and `/api` are added only once a Capability exists: `new-module.sh` adds them, with a comment saying why, when it creates the first.
   - `new-module.sh` takes `CAPABILITY=<capability>` (required; `make new-module CAPABILITY=crm NAME=contacts`). It creates `crates/capabilities/<capability>/` and its `capability.yaml` (label from the id in words) when it's the first Module there, and refuses a Module name already taken in any Capability, the platform or the reference.
   - `check-modules.sh`, `check-changed.sh` and `check-sql-schemas.sh` cover `crates/capabilities/*/*` as they do `crates/modules/*` today, and drop `crates/modules/`.
   - `check-modules.sh` fails on:
     - a Module with no `capability.yaml` above it;
     - a Capability folder with no Module;
     - a Capability with no entry in the SPA's registry, apart from `assistant`, which has no App;
     - a registry entry naming no Capability.
5. **The SPA's registry is keyed by Capability.** Rename `shell/apps.ts` to `shell/capabilities.ts`: `ShellApp` becomes `Capability`, `useShellApps` becomes `useCapabilities`, and each entry gains `id`, its Capability's. Update every import, `frontend/AGENTS.md`'s Shell section and the screens. `check-modules.sh` reads the ids from this file, so keep each as a literal `id: "<id>"`.
6. **`make init`.**
   - `scripts/init.sh` writes `capabilities.ts` (with no entries) where it wrote `apps.ts`, and leaves out `crates/capabilities/` and its globs.
   - `check-init` runs `make new-module CAPABILITY=crm NAME=contacts` in the scratch app where it ran `NAME=contacts`, and checks that its tests pass and `check-modules` passes. That needs the scaffold to add the registry entry, so `new-module.sh` appends `{ id: "<capability>", … }` with one Area for the new Module when the Capability is new.
7. **Docs.**
   - `docs/platform/glossary.md`:
     - add **Capability**;
     - **App** is a Capability's, and the line letting one Module serve several Apps goes;
     - "capability trait" becomes "Module API trait".
   - `crates/AGENTS.md`: paths, "Adding a module", and the reference table where it names `crates/modules/`.
   - The root `AGENTS.md`, if it names `crates/modules/`.
   - The `new-module` skill.

## Acceptance criteria

- Every Module's Capability is the folder it sits in. `make check-modules` fails on each mismatch listed in step 4.
- The Catalog and the access screens group resources by Capability.
- `make new-module CAPABILITY=crm NAME=contacts` in a fresh `make init` app scaffolds `crates/capabilities/crm/contacts/` and a registry entry, and its tests pass. `make check-init` passes.
- `make check-changed` passes, including e2e.

## Notes

Areas' `needs` and `get_my_permissions` are 0078.
