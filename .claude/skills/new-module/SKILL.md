---
name: new-module
description: Add a backend module (a new bounded context, crate or aggregate's home) to the modular monolith with `make new-module`, register one by hand, or add a read module. Use when the user wants a new module, a new set of operations with their own tables, or asks how modules are registered.
---

# Adding a module

Extends `crates/AGENTS.md`, which you must have read: its layers, aggregates, across-modules and test rules all apply to the new module. If this file and the code disagree, the code wins: fix this file in the same change.

## Scaffold

`make new-module CAPABILITY=crm NAME=contacts [ENTITY=<aggregate>]` (Postgres up: `make db-up`) copies `crates/module-template/{core,api}` (workspace crates, packages `widgets` and `widgets-api`, compiled and tested but registered with no server) into `crates/capabilities/crm/contacts/`, a module that compiles, passes its tests and is registered everywhere: every layer, a resource policy with no rules, default grants for `user` and `admin` and an `OPERATIONS` table, a versioned aggregate with create/read/rename/list publishing its events, a projection of its own topic behind a search operation, migrations, seed and tests. It allocates the next module number, applies the migrations to the dev DB and refreshes `.sqlx`. It refuses a module name already taken in any Capability, the platform (`access`, `assistant`) or the reference, and a Capability id another Capability has.

**The Capability** (ADR 0047) is the product the module belongs to: the one whose data it keeps, where a person goes to change it. Every module is in exactly one, whose folder it sits in; pick an existing Capability unless it is a new product. The first module of a Capability also makes `crates/capabilities/<capability>/capability.yaml` (`{ id, label }`, the label the id in words: fix it for admins), the workspace's `crates/capabilities/*/*/core` and `/api` members if it is the first Capability, and its App: an entry `{ id: "<capability>", … }` in `frontend/src/shell/capabilities.ts` with one Area, the module's, routed in `src/router.tsx` to a `src/pages/<capability>-<module>-page.tsx` saying it isn't built yet. Write the App's description and build the Area's feature (`frontend/AGENTS.md`). Same Capability is no licence to couple: its modules still meet only through their `-api` crates.

## Then

1. Reshape the aggregate, keeping its migration, seed and tests in step; `make prepare` after changing a `query!`. Each operation keeps its guard (the `endpoint-auth` skill): an action in the resource's enum, a default grant, a row in `OPERATIONS`, `self.authorizer.authorize(..)` in the use case.
2. Keep the search projection only if the module needs a read shape its tables can't serve, point it at another module's topic to search by that module's columns (the `events-and-projections` skill), or delete it: its migration, `db/search.rs`, `handlers(..)`, the operation and its tests.
3. If it uses another module, depend on that module's `-api` and pass the provider's constructor to its `module(..)` in `crates/server/src/modules.rs`, after the module it uses.
4. Write each operation's `operation_id` and summary for an agent as much as a person: they are its MCP tool's name and description.
5. `make generate` and commit `frontend/openapi.json`; for a UI, follow `frontend/AGENTS.md`.
6. If it reacts to, provides for, or is read by another module, add or extend a journey in `crates/server/tests/journeys/`.
7. Done when `make check-changed` passes (`make check` before merging).

## Registering by hand

A read module isn't scaffolded: copy `crates/reference/note-search`. Registered by hand or not, a module is listed in:

- Its Capability's folder: `crates/capabilities/<capability>/<module>/`, beside a `capability.yaml`, which its `policies()` names: `.capability(policy_file!("../../capability.yaml"))`. A new Capability also needs its entry in `frontend/src/shell/capabilities.ts` and, if it is the first, the `crates/capabilities/*/*/core` and `/api` workspace members.
- `crates/server/src/modules.rs`: the `Endpoints` tuple and `modules()`. Past 12 modules, nest sub-tuples in the `Endpoints` tuple (std's `Default` cap; poem-openapi's is 16).
- `crates/server/Cargo.toml`: the dependency, and the `seed` feature if it has a seed.
- The root `Cargo.toml` workspace dependencies: core and `-api`.
- Both `deny.toml` `wrappers` lists (`sqlx` and `poem`): the core, never the `-api`.
- A per-crate `clippy.toml` (copy a sibling's).
- `pub const MODULE: u32` in its seed, unique across modules.

`make check-modules` fails on any missing entry, a module with no `capability.yaml` above it or not naming it, a Capability folder with no module, and a Capability with no entry in the SPA's registry (`assistant` has none) or an entry with no Capability; `make check-deps` on a core depending on another core or an `-api` on anything impure.
