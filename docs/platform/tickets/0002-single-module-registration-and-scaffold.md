---
id: 0002
name: Register each module in one place, scaffold new ones, test the composed server
status: done
priority: P1
size: M
blocked_by: []
blocks: [0007]
source: architecture review 2026-10-31, finding 2
---

# Register each module in one place, scaffold new ones, test the composed server

## Description
Adding a module is the template's core workflow, but it takes about 9 hand edits
across shared files. Several omissions fail silently: the code compiles, every
module's own tests pass, and prod breaks. An agent working on one module also has
to understand five shared files.

## Evidence (as of 2026-10-31)
Edit sites for a new module:
- root `Cargo.toml`: members and workspace deps
- `deny.toml`: two wrapper lists
- `crates/server/Cargo.toml`: the dependency and the `seed` feature
- `crates/server/src/lib.rs`:
  - the `Endpoints` tuple *type*
  - the tuple *value* in `build_service()`
  - `module_migrations()`, ordered by hand
- `crates/server/src/main.rs`: `.accept(provider(..))`
- `crates/server/src/bin/seed.rs`
- the module-number registry in `spec.md` §13
- copies of `clippy.toml`, `build.rs` and the driver skeleton

Silent failures:
- **Missing `.accept`:** services resolve at runtime via `Data<&XService>`, so every
  request to the module returns 500 in prod.
- **Missing `module_migrations()` entry:** the module's tables are never created in
  prod, because test drivers migrate for themselves.
- **Missing seed entry:** the dev DB is empty for that module.
- `crates/server` has no test that boots the composed app.

## Proposed approach
1. **One registration point per module.** Each module core exports
   `pub fn module(..) -> ModuleDef` (or a set of free functions with one fixed
   signature) providing its provider factory, migrations, and seed (behind
   `seed`). `server` keeps one ordered list of modules and iterates it for
   `.accept`, migrations and seeding. The `Endpoints` tuple stays one type alias,
   used for both the type and the value.
2. **Composed-app smoke test.** Extract a shared `build_app(pool, verifier, ..)`
   from `main.rs`. Add one `#[sqlx::test]` in `server` that migrates through
   `module_migrations()`, seeds, builds the real app and issues one GET per module,
   expecting non-5xx.
3. **Scaffold.** `make new-module NAME=foo [KIND=write|read]` generates the crate
   (layers, `clippy.toml`, `build.rs`, seed, driver and one test) and either
   patches the shared lists or prints the exact remaining edits.
4. Move the module-number registry next to the code (e.g. a const in each seed,
   checked for uniqueness), or have the scaffold allocate the next number.

## Acceptance criteria
- [x] Forgetting to register a module's provider or migrations fails `make check`.
- [x] `make new-module NAME=widgets` produces a module whose tests pass unmodified.
- [x] Shared files a human must edit for a new module: ≤ 2.
- [x] `CLAUDE.md` describes adding a module as one command plus a short checklist.

## Open questions
- `ModuleDef` struct vs. a trait vs. a macro: poem-openapi's endpoint tuple is a
  compile-time type, so endpoints probably stay outside the struct. Decide after a spike.
- Should the scaffold edit `deny.toml`/`Cargo.toml` itself, or should those lists be
  derived (e.g. a deny-config generator)?
- Is cargo-generate worth the extra tool, or is a shell script enough?

## Resolution
- **Registration:** a `Module` struct in a new coupled crate, `app-module` (poem +
  `app-db`). Each module core returns one from `di.rs` (`users::module()`,
  `notes::module(users::directory)`), bundling its provider, migrations and seed.
  `crates/server/src/modules.rs` is the single registry: the `Endpoints` type alias
  (its value is `Endpoints::default()`) and the ordered `modules()` list.
  `module_migrations()`, `build_app()`'s request data and the `seed` bin all derive
  from that list. Endpoints stay outside the struct, as expected.
- **Composed-app test:** `build_app()` is in `crates/server/src/app.rs`, shared by
  `main.rs` and `crates/server/tests/composed_app.rs`. The test sends a GET for every
  path in the spec, filling path parameters with a nil uuid and sending a bearer
  token. It fails on any 5xx and on a tag that has no GET. Dropping a provider shows
  up as `answered 500`; dropping migrations fails at seeding.
- **Registration lint:** `scripts/check-modules.sh` (`make check-modules`, in
  `make check` and CI) fails when a module core crate is missing from `modules.rs`,
  from `crates/server/Cargo.toml` (dependency and `seed` feature) or from either
  `deny.toml` list, or when two modules share a module number.
- **Scaffold:** a shell script (`scripts/new-module.sh`) over a template that is real
  Rust (`scripts/new-module/template/`, a `widgets` module). cargo-generate wasn't
  worth the extra tool. The script patches the shared lists itself rather than
  deriving them. Workspace `members` became globs, so that list needs no entry. The
  new module's `query!`s can't compile offline before `.sqlx` knows them, and
  `migrate` can't build until they do. So the script applies the new migrations
  with `sqlx-cli` into the module's own schema (where `make migrate` later finds
  them applied), then runs `sqlx prepare`. A CI job scaffolds a module and runs its
  tests, since nothing else compiles the template. Only write modules are
  scaffolded; read modules copy `note-search`.
- **Module numbers:** each seed declares `pub const MODULE: u32` and builds its ids
  with `app_domain::well_known_id(MODULE, kind, n)`. The lint checks the numbers are
  unique and the scaffold allocates the next one. The table in spec §13 is gone.
- Shared files a human edits for a new module: none for a standalone write module;
  for one that uses another module, only `modules.rs`, to pass the capability.
