# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

A clone-and-go **template**: a Rust modular-monolith backend (Poem + poem-openapi) whose OpenAPI spec drives a fully type-safe Vite + React + TypeScript frontend. `spec.md` is the authoritative design doc and `CONTEXT.md` is the glossary — read them when a decision isn't obvious here. Keep design decisions minimal; this is a template, not a product.

## Commands

`make` is the orchestrator; its dependency graph handles cross-stack builds (real file targets do the work, phony targets are aliases).

- `make setup` — **run once after cloning.** Installs the repo-local Rust CLI tools. See "Dev tooling" below.
- `make dev` — migrate + generate client, then run backend (`:8080`) and Vite (`:5173`) in parallel.
- `make check` — the CI mirror: `fmt` + `clippy` + `deny` + backend `test` + FE `typecheck`. Run this before considering work done.
- `make generate` — `dump-spec` → `frontend/openapi.json` → hey-api codegen → `frontend/src/api/`. Run after any backend API change.
- `make migrate` / `make prepare` — apply migrations / refresh the checked-in `.sqlx` offline cache (needs a migrated DB).
- `make build` — `vite build` then `cargo build --release --features embed-frontend` (embeds the SPA into the binary).
- `make db-up` / `make db-wait` / `make db-down` — local Postgres via docker-compose (also the test server).

**Backend tests** use `#[sqlx::test]` against the **docker-compose Postgres**: each test gets a fresh `_sqlx_test_*` database on that server. Run them via make, which brings compose up and points `DATABASE_URL` at the `postgres` database (sqlx writes a `_sqlx_test` bookkeeping schema there — keep it out of the dev `app` DB) with `SQLX_OFFLINE=true` (otherwise `query!` compiles against that unmigrated DB and fails with `relation "users.account" does not exist`).
- All: `make test` (`make check` includes it)
- One crate / one test: `make db-wait`, then `SQLX_OFFLINE=true DATABASE_URL=postgres://app:app@localhost:5432/postgres cargo test -p users registering::` (any substring of the test path, e.g. a behaviour file or test name)
- A test hanging ~30s then panicking `PoolTimedOut` means Postgres isn't up — run `make db-up`.
- `make test-db-clean` drops per-test databases left behind by failed or renamed tests. Two test runs against the same server at once (e.g. two worktrees, or cargo-watch alongside `make test`) collide — database names are derived from the test path, not random.

**Frontend** (run from `frontend/`, uses `pnpm`):
- Single test: `pnpm vitest run src/App.test.tsx -t "lists notes"`
- `pnpm run typecheck` (`tsc -b --noEmit`), `pnpm biome ci .` (lint/format), `pnpm run test` (Vitest).

## Dev tooling (repo-local, not global)

Rust CLI tools (`cargo-deny`, `cargo-watch`, `sqlx-cli`) are declared with pinned versions in the root `Cargo.toml` `[workspace.metadata.bin]` and installed into a gitignored `./.bin` by `cargo-run-bin` — nothing goes into global `~/.cargo/bin` except the one bootstrap tool. `make setup` installs `cargo-run-bin` and then `cargo bin --install`. In the Makefile these tools are invoked as `cargo bin <tool>` (e.g. `cargo bin cargo-deny check`); don't call a globally-installed `cargo deny`/`cargo watch`/`sqlx` — it may be a different version. To bump a tool, edit the version in `Cargo.toml` (use a range qualifier like `^0.20`, not a bare `0.20`) and re-run `make setup`. Frontend tools are the equivalent, already repo-local via `frontend/package.json` + `node_modules`. `lefthook` is the one exception (a Go binary): install it separately (`brew install lefthook`, then `lefthook install`).

## Non-negotiable architecture rules

These are what make the template what it is. Violating them defeats its purpose.

**Module isolation and layering (spec §4, D6).** Each module (`crates/modules/*`) is its own crate with four layers as sub-module **folders**, each split into files and re-exported from its `mod.rs` (callers use `crate::domain::User`, never `crate::domain::user::User`). Dependencies point inward: `api → application → domain`, and `db → application`. Flow for a use case: **handler → service → repository**.
- `api/` — `handlers.rs` (`#[OpenApi]` impl on a **unit struct**; gets the pool via `poem::web::Data<PgPool>`, never struct state; turns requests into domain **commands**, calls the service, maps results to HTTP; no business logic) and `models.rs` (request/response `Object`s + `From<aggregate>` conversions).
- `application/` — `service.rs` (`XService` holding an `Arc<dyn XRepository>` — a trait object, not a generic parameter — one method per use case: **load aggregate → domain action → `save`**; owns cross-aggregate rules like uniqueness, and mints ids and reads the clock), `repository.rs` (the **`Repository` trait** — lives here, not in the domain; speaks in aggregates, never rows: `get → Option<Aggregate>`, `save(&Aggregate)`), `error.rs` (exhaustive `From<XError> for AppError`). Depends only on the domain and pure crates.
- `domain/` — the aggregate (`user.rs`: private fields, getters, constructor/actions taking a command and returning `Result<_, XError>`, plus `restore(..)` for repositories), value objects (`email.rs`), `commands.rs`, `error.rs` (module-specific `thiserror` enum), `ids.rs`. Pure and synchronous: no repositories, no async, no `Utc::now()`, no `XId::new()` — constructors take the id and `now` as arguments, so the service passes fresh ones and seeds pass well-known ones. Never names `poem` or `sqlx`.
- `db/` — `repository.rs` (the `sqlx` impl of the application's Repository trait; `save` is an upsert; the only place raw SQL lives) and `migrations.rs` (`SCHEMA`, `MIGRATOR`); owns the module's `migrations/` dir (its own Postgres schema).
- `seed.rs` — behind the crate's `seed` feature: the module's well-known test dataset (see Conventions).
- `lib.rs` keeps every layer private and re-exports only the `Api`, public domain types, and the migration bundle (services and repositories stay internal — tests go through the `Api`) (plus `pub mod seed` under the feature).

As a layer grows, add files by concern (e.g. `api/handlers/` per resource, `application/service/` per use case, one file per aggregate in `domain/`) rather than growing one file. The `#![allow(clippy::disallowed_types)]` lives in `db/mod.rs` (covers the whole db layer), `api/handlers.rs` (pool extraction only) and `seed.rs` (wires a pool to the repository); test files also allow it to receive the per-test `PgPool` — never add it under `application/` or `domain/`.

Isolation is enforced by three mechanisms, all of which must keep passing: (1) the **Cargo graph** — a module without a sibling in `[dependencies]` cannot import it; (2) **cargo-deny** (`deny.toml`) bans `sqlx` workspace-wide except an allow-list of db-coupled crates — do not add a module to that list unless it genuinely has a `db/` layer; (3) **per-crate `clippy.toml`** bans naming `sqlx::PgPool` etc. outside the db layer (run clippy with `-D warnings`) — a tripwire only: other sqlx types, `poem`, and a local `#[allow]` get past it.

**`common` crates split on pure vs framework-coupled (spec §5, D2).** `app-test-support` is dev-only and pure (JSON path assertions); keep poem/sqlx out of it — transport code lives in each module's test driver. Never merge them into one `common`. A pure crate (`app-config`, `app-error`) carries no poem/sqlx dep so the domain can use it; a coupled crate (`app-web` = poem, `app-db` = sqlx) may only be used by the layer sharing that concern. The `app-error` (pure) → `app-web` (HTTP mapping) split is deliberate: the application layer returns `app-error` values with zero poem knowledge.

**The spec is the source of truth for the frontend (spec §7–8).** The FE client in `frontend/src/api/` is **generated, never hand-edited**. Flow: backend Rust → `dump-spec` (DB-free, sub-second — works because Api structs are unit structs and the pool is injected) → `frontend/openapi.json` → hey-api codegen. A backend API change that breaks the FE surfaces as a **TypeScript compile error** — that's the safety net; keep it working. `build_service()` in `crates/server/src/lib.rs` is shared by the running server and `dump-spec` so the served spec matches the dumped one byte-for-byte. Mind poem-openapi's 16-element `OpenApiService` tuple cap when adding modules.

**Migrations run explicitly, never on startup (spec §6).** `make migrate` (via `server/src/bin/migrate.rs`) applies per-module migrations in dependency order. `SQLX_OFFLINE=true` is the default so `cargo build` and CI need no live DB; regenerate the `.sqlx` cache with `make prepare` after changing any `query!`.

## Conventions

- **Pinned stack** (spec §2): keep `poem` `3.1.x` ↔ `poem-openapi` `5.1.x` paired (released in lockstep by date). Effective workspace MSRV is **1.94** (sqlx 0.9), enforced by `rust-version` in the root `Cargo.toml`. hey-api packages are pinned exact (pre-1.0).
- **Testing is integration-first (spec §13, D3).** Test **outside-in**: a module's behaviour is tested only through its public HTTP surface, through every real layer, against real Postgres. Each module has one test target, `tests/<module>/`: `main.rs` (declares the files), `driver.rs`, and one file per behaviour area (`registering.rs`, `editing.rs`, …). The **driver** is the only code that knows the transport. It boots the `Api` over the per-test pool (`Notes::seeded(pool)`: migrate, seed, build), exposes domain verbs (`notes.edit(FIRST_NOTE, ..)`, `users.signed_in_as(ALICE).me()`), and turns responses into `Result<View, Refusal>` (`Refusal::Invalid | NotFound | Conflict | NotSignedIn`, only the variants the module uses). A view is a newtype over the returned `serde_json::Value` (`Deref`s to it, with `From<Aggregate>` building the expected JSON): compare it whole against a seed when every property is known, and use `app-test-support`'s JSON path macros (`assert_json_matches!(note, "$.title", "Third")`, `assert_json_exists!` for generated ids/timestamps, `assert_json_not_exists!` for fields that must not leak) when some aren't. **Tests use functional language, not HTTP lingo**: names describe behaviour (`an_edit_without_a_title_is_rejected_and_changes_nothing`), bodies call driver verbs and compare against seed aggregates (`Ok(seed::alice().into())`), and never mention routes, methods, or status codes (JSON paths naming returned properties are fine). One behaviour per test (a fresh DB per test is cheap), and check that refused actions changed nothing. Every DB test is `#[sqlx::test(migrations = false)] async fn name(pool: PgPool)`. Don't test services directly unless a use case has no HTTP entry point (e.g. a job or consumer) or two outcomes are indistinguishable over HTTP. **`migrations = false` is mandatory** — without it the macro applies `migrations/` itself into `public`, bypassing the schema-per-module runner. `tests/<module>/main.rs` carries `#![allow(clippy::disallowed_types)]` to name `PgPool`. Don't write in-memory Fakes for Postgres. Reserve inline unit tests for pure domain logic (aggregates, value objects) — use `proptest` when the state space is combinatorial. A hand-written **Fake** is only for an external dependency that can't be run locally.
- **Seeding is first-class (spec §13).** Every module owns `src/seed.rs` behind a `seed` cargo feature (enabled for its own tests by a self dev-dependency, never in prod): named `const` ids (`users::seed::ALICE`), one `fn` per well-known aggregate (`seed::alice()`), and `seed::standard(&pool)` which saves the dataset via the real repository. Tests start from a seeded driver and refer to seeded entities by constant — never capture ids from responses to find seeded data. Seeds are built with the real domain constructors (so they satisfy invariants) at fixed timestamps (so ordering is deterministic). Ids follow `MMMMMMMM-KKKK-0000-0000-NNNNNNNNNNNN` (module number, entity kind, sequence); the module registry is in spec §13. Seeds never reference another module's seed ids.
- **FE tests mock at the network boundary with MSW only** (spec §13) — never module/import mocks — so the real generated client stays in the path. Note: the client's prod `baseUrl` is a relative `/api`; Node's fetch rejects relative URLs, so `vitest.setup.ts` gives the real client an absolute base for tests.
- **Prod is a single artifact.** The SPA is embedded into the binary via `rust-embed` behind the `embed-frontend` feature (off by default so the backend builds before the FE exists). `make build` and the Dockerfile enable it; dev instead uses the Vite proxy (`/api` → `:8080`).
- **Auth is a minimal seam (spec §11).** One JWT Bearer `SecurityScheme` in `app-web` guards one route in `modules/users`. No user store or token issuance — don't build one unless asked.

## IDs and timestamps over the wire

Domain uses real types (`uuid::Uuid`, `chrono`), but HTTP request/response types expose ids and timestamps as plain **`string`** so the generated TS client stays dependency-free. Preserve this when adding fields.
