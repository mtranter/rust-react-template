# Template: Rust modular-monolith backend + Vite/React/TS frontend

A clone-and-go template for full-stack projects. A Rust modular-monolith backend
(Poem + poem-openapi) whose OpenAPI spec drives a fully type-safe Vite + React +
TypeScript frontend. First-class local DX; a single-artifact production deploy
with no architectural compromise.

Terminology in this document follows [`CONTEXT.md`](./CONTEXT.md).

---

## 1. Goals & non-goals

**Goals**
- Modular monolith where each **module** is genuinely isolated and **liftable**
  into a microservice.
- End-to-end type safety: Rust types → OpenAPI **spec** → generated TypeScript
  **client**, with a fast inner loop.
- First-class DX (`make dev` does everything) without compromising the
  production deployment architecture.
- Production artifact: a single self-contained binary.

**Non-goals (documented seams, not built by default)**
- No auth beyond a minimal JWT example (no user store, no session backend).
- No OpenTelemetry exporter wired (seam only).
- No end-to-end browser tests (Playwright) by default.
- No S3/CloudFront static-hosting deploy path (the embedded-binary path is the
  default; static hosting is a documented future variant).

---

## 2. Tech stack (pinned)

| Area | Choice | Version |
|---|---|---|
| Web framework | `poem` | `3.1` (MSRV 1.85, edition 2024) |
| OpenAPI | `poem-openapi` | `5.1` (features: `swagger-ui`) |
| Async runtime | `tokio` | `1` |
| Database | PostgreSQL + `sqlx` (macros, offline mode) | `sqlx` `0.9` |
| Frontend embed | `rust-embed` via `poem` `embed` feature | — |
| Errors | `thiserror` | `2` |
| Config | `figment` (env + file layering) | latest |
| Tracing | `tracing` + `tracing-subscriber` | latest |
| Integration test infra | `#[sqlx::test]` (per-test database) against docker-compose Postgres | (sqlx) |
| Property tests | `proptest` | latest |
| Frontend build | Vite + React + TypeScript | latest |
| Type codegen | `@hey-api/openapi-ts` + `@tanstack/react-query` plugin + `@hey-api/client-fetch` | pin exact (pre-1.0) |
| Data fetching | `@tanstack/react-query` | `5` |
| Package manager | `pnpm` | — |
| FE lint/format | Biome | latest |
| FE tests | Vitest + React Testing Library + MSW | latest |
| Task runner | GNU `make` | — |
| Arch enforcement | Cargo graph + `cargo-deny` + per-crate `clippy.toml` | `cargo-deny` `0.20` |
| Git hooks | `lefthook` | latest |
| Prod runtime image | `gcr.io/distroless/cc` | — |

> Poem and poem-openapi are released in lockstep by date, not by shared version
> number — keep the `poem` `3.1.x` ↔ `poem-openapi` `5.1.x` pairing.
>
> Although `poem` declares MSRV 1.85, **`sqlx` `0.9` requires Rust `1.94`**, so
> the effective workspace MSRV is `1.94` (`rust-version` in the root
> `Cargo.toml`). `rust-toolchain.toml` pins the latest stable.

---

## 3. Repository layout

```
repo/
├── Cargo.toml                # workspace: members = crates/{common,modules,server}/*
├── Cargo.lock
├── rust-toolchain.toml       # pin latest stable; MSRV floor 1.94 (sqlx 0.9)
├── deny.toml                 # cargo-deny bans (keep sqlx out of non-db layers)
├── .sqlx/                    # checked-in sqlx offline query cache
├── Makefile
├── docker-compose.yml        # local Postgres
├── Dockerfile                # multi-stage prod build
├── lefthook.yml
├── CONTEXT.md
├── spec.md                   # this file
├── .github/workflows/ci.yml
│
├── crates/                   # all Rust workspace members
│   ├── modules/
│   │   ├── users/            # example module (with the JWT-guarded route)
│   │   │   ├── Cargo.toml
│   │   │   ├── clippy.toml   # disallowed-types: sqlx::* banned outside db
│   │   │   ├── migrations/   # module-owned schema (schema `users`)
│   │   │   └── src/
│   │   │       ├── lib.rs    # re-exports Api, service, repo trait, domain types
│   │   │       ├── api/          # API layer
│   │   │       │   ├── mod.rs        # re-exports the Api
│   │   │       │   ├── handlers.rs   # #[OpenApi] impl (unit struct) → commands
│   │   │       │   └── models.rs     # request/response Objects
│   │   │       ├── application/  # application layer
│   │   │       │   ├── mod.rs        # re-exports service + repo trait
│   │   │       │   ├── service.rs    # use cases: load → domain action → save
│   │   │       │   ├── repository.rs # Repository trait (aggregates in/out)
│   │   │       │   └── error.rs      # domain error → AppError
│   │   │       ├── domain/       # domain layer (pure, sync)
│   │   │       │   ├── mod.rs        # re-exports the public domain surface
│   │   │       │   ├── user.rs       # the aggregate + domain actions
│   │   │       │   ├── email.rs      # value object
│   │   │       │   ├── commands.rs   # intents accepted by domain actions
│   │   │       │   ├── error.rs      # domain error enum
│   │   │       │   └── ids.rs        # newtype identifiers
│   │   │       └── db/           # DB layer
│   │   │           ├── mod.rs        # re-exports repo impl + migrations
│   │   │           ├── migrations.rs # SCHEMA + embedded MIGRATOR
│   │   │           └── repository.rs # sqlx Repository impl (save = upsert)
│   │   └── notes/            # second module (proves cross-module isolation)
│   │       └── … (same shape, schema `notes`)
│   │
│   ├── common/
│   │   ├── config/           # pkg: app-config     — pure
│   │   ├── error/            # pkg: app-error      — pure
│   │   ├── telemetry/        # pkg: app-telemetry  — tracing + OTEL seam
│   │   ├── web/              # pkg: app-web        — poem-coupled
│   │   ├── db/               # pkg: app-db         — sqlx-coupled
│   │   └── test-support/     # pkg: app-test-support — dev-only, pure test utilities
│   │
│   └── server/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs       # assemble service, middleware, embed FE, serve
│           └── bin/
│               ├── dump-spec.rs  # print OpenAPI JSON to stdout (DB-free)
│               └── migrate.rs    # run all module migrations in order
│
└── frontend/
    ├── package.json
    ├── pnpm-lock.yaml
    ├── vite.config.ts        # dev proxy /api -> :8080
    ├── openapi-ts.config.ts  # hey-api codegen config
    ├── biome.json
    ├── tsconfig.json (+ app/node references)
    ├── openapi.json          # spec snapshot (generated; input to codegen)
    └── src/
        ├── api/              # generated client (never hand-edited)
        ├── main.tsx
        └── App.tsx
```

**Naming**: crate *folders* under `common/` are short (`config`, `web`…) but
package **names** are prefixed `app-*` to avoid crates.io collisions and read
clearly at the `use` site.

---

## 4. Modules & layers

Each module is one crate exposing four internal layers as sub-module
**folders**. Each layer splits its contents across files by concern and
re-exports them from its `mod.rs`, so the rest of the crate addresses a layer
(`crate::domain::User`), never a file inside it. Growing a layer means adding
files (or nested folders, e.g. `api/handlers/` per resource), not growing one.

Dependencies point inward — `api → application → domain`, with `db` implementing
the application's `Repository` — and every use case flows **handler → service →
repository** (see D6):

```
handler:  request ──► command ──► service.method(command)
service:  repo.get(id) ──► aggregate.action(command)? ──► repo.save(&aggregate)
```

- **`api/`** — `handlers.rs`: a `#[OpenApi]` impl on a **unit struct** (no
  fields). Operations obtain the pool via `poem::web::Data<PgPool>`, not via
  struct state, build a **command** from the request, call the service, and map
  the result (an `AppResult`) to HTTP via `app-web`. No business logic.
  `models.rs`: request/response `Object`s and their conversions from aggregates.
- **`application/`** — `service.rs`: one service per aggregate
  (`UserService`, holding an `Arc<dyn UserRepository>` rather than being generic
  over it), one method per use case. A service loads an
  aggregate from the repository, runs a domain action on it, and saves it. It
  owns what a single aggregate cannot: rules spanning many aggregates (e.g. email
  uniqueness), not-found semantics, fresh ids, and the clock. `repository.rs`: the
  module's **`Repository`** trait — it speaks in aggregates, never rows
  (`get → Option<Aggregate>`, `save(&Aggregate)`). `error.rs`: an exhaustive
  mapping from the domain error onto `AppError`. Depends only on the domain and
  pure crates.
- **`domain/`** — the aggregate (private fields, getters, a constructor and
  actions that take a command and return `Result<_, UserError>`, and a
  `restore(..)` constructor repositories use to rebuild persisted state), value
  objects validated on construction (`Email`), commands, the module's own
  `thiserror` error enum, and newtype ids. Pure and synchronous: no repository,
  no async, no clock, no id minting — constructors take the id and `now`, so the
  same code path serves the service (fresh values) and seeds (well-known values).
  Depends only on pure crates
  (`app-config`, `app-error`, `chrono`, `uuid`, `thiserror`) — never on `poem`
  or `sqlx`.
- **`db/`** — `repository.rs`: the `sqlx` implementation of the `Repository`
  trait, the only place raw SQL lives; `save` is an upsert on the primary key.
  `migrations.rs`: the module's schema name and embedded migrator. Owns the
  module's `migrations/` (its own schema).

`lib.rs` keeps every layer private and re-exports the module's `Api`, its
public domain types, and its migration bundle; nothing else is visible outside
(services and repositories are internal — tests go through the `Api`, §13).

### Isolation enforcement (layered defenses)

1. **Cargo workspace graph (primary).** A module crate that does not list a
   sibling in `[dependencies]` physically cannot import it. This carries ~80% of
   the isolation for free and is stronger than a test — it won't compile.
2. **`cargo-deny` `[bans]`.** `sqlx` is denied workspace-wide with a `wrappers`
   allow-list permitting only `app-db` (and module `db` layers) to depend on it.
   Bans target third-party crates; the Cargo graph handles internal edges.
3. **Per-crate `clippy.toml` (`disallowed-types`/`disallowed-methods`).** In each
   module, bans `sqlx::PgPool`, `sqlx::Pool` and `sqlx::Postgres` from being
   *named* anywhere in the crate; `db/mod.rs` and `api/handlers.rs` opt out, so
   the application and domain layers cannot. Run with `-D warnings`. This is a
   tripwire, not a boundary: other sqlx types, `poem`, and a local `#[allow]`
   all get past it.
4. **`rust_arkitect`** — noted for within-crate layering assertions, but not
   wired: crate-per-module makes it largely unnecessary.

---

## 5. Common crates

The split axis is **pure vs framework-coupled** — a pure crate can be used by the
domain without dragging in poem/sqlx, which is what lets the bans in §4 hold.

| Crate | Coupling | Contents | Who depends |
|---|---|---|---|
| `app-config` | pure | `figment`-layered env/file → typed `AppConfig` | anyone |
| `app-error` | pure | `thiserror` canonical error kinds | anyone (incl. domain, application) |
| `app-telemetry` | tracing/OTEL | `tracing-subscriber` init (JSON prod / pretty dev), OTEL seam | server, modules |
| `app-web` | poem | `app-error` → `ApiResponse` mapping, request-id middleware, `/health`, pagination types, **JWT `SecurityScheme`** | api layers, server |
| `app-db` | sqlx | `PgPool` construction, migration runner, `Repository` conventions | db layers, server |
| `app-test-support` | dev-only, pure | JSON path assertions (`assert_json_matches!`, `assert_json_exists!`, `assert_json_not_exists!`) | module dev-deps only |

The **`error` (pure) vs `web` (poem mapping)** split is deliberate: application
services return `app-error` values with zero poem knowledge; `app-web` owns the
HTTP translation. (Domain actions return the module's own error enum, which the
application layer maps onto `app-error`.)

---

## 6. Database, migrations, offline compile

- **PostgreSQL**, accessed with **`sqlx` macros** (`query!`) for compile-time
  checking. Local Postgres via `docker-compose.yml`.
- **Schema-per-module**: each module writes to its own Postgres schema
  (`users`, `notes`, …).
- **Per-module migrations**: each module crate owns a `migrations/` directory.
  `server/src/bin/migrate.rs` applies them in a deterministic, dependency-aware
  order via the `app-db` runner.
- **Offline compile**: the `.sqlx/` query cache is checked in and
  `SQLX_OFFLINE=true` is the default, so `cargo build` and CI need **no live
  database**. `make prepare` regenerates the cache against the docker Postgres.
- **Migrations run explicitly** via `make migrate` (which `make dev` depends on)
  — never silently on server startup, so production controls migration timing.

---

## 7. OpenAPI spec generation

- The **code is the source of truth**. `server` builds an `OpenApiService` from
  the tuple of every module's `Api` (mind poem-openapi's 16-element tuple cap —
  group into sub-tuples past 16 modules).
- `OpenApiService::spec()` returns the spec JSON **synchronously, with no server
  and no async runtime**. `dump-spec.rs` constructs the *same* service
  configuration (shared `build_service()` fn — identical `title`/`version`/
  `server` url as runtime) and prints the JSON to stdout.
- Because handlers take the pool via `Data<PgPool>` and `Api` structs are unit
  structs, **`dump-spec` needs no database and no pool** — spec generation is a
  pure, sub-second step. This is the linchpin of the fast inner loop.
- At runtime the server also serves the spec at `/api/spec.json` and Swagger UI
  at `/docs`.

---

## 8. Frontend & the type-generation loop

- **Vite + React + TypeScript**, `pnpm`, Biome, Vitest.
- **Codegen**: `@hey-api/openapi-ts` reads `frontend/openapi.json` (static file,
  no running server needed) and generates into `frontend/src/api/`:
  the typed client (`@hey-api/client-fetch`) and TanStack Query artifacts
  (`...Options()` / `...Mutation()` spreadables composed with your own
  `useQuery`/`useMutation`).
- Client `baseUrl: "/api"` so identical code works in dev (Vite-proxied) and
  prod (same-origin).
- **Loop**: backend source → `dump-spec` → `frontend/openapi.json` → hey-api
  codegen → `frontend/src/api/` → `tsc -b --noEmit` typecheck. A backend API
  change that breaks the frontend surfaces as a **TypeScript compile error**.

---

## 9. Serving: dev vs prod

**Dev (first-class DX):**
- Vite dev server on `:5173` with HMR, `server.proxy` forwards `/api` →
  Rust on `:8080`.
- Rust runs headless (API only). CORS allows the Vite origin.

**Prod (single artifact, no compromise):**
- `vite build` → `frontend/dist` embedded into the Rust binary via `rust-embed`
  (`EmbeddedFilesEndpoint`), with a manual catch-all fallback to `index.html`
  for client-side routing (the embedded endpoint has no built-in fallback
  toggle).
- One binary serves both the API and the SPA. No separate static host.

---

## 10. Dev experience — `make`

`make` is the orchestrator and uses its **dependency graph** for cross-stack
builds: real file targets do the work, phony targets are ergonomic aliases.

Dependency edges (real targets):
- `frontend/node_modules` ⟵ `frontend/pnpm-lock.yaml`, `frontend/package.json`
  → runs `pnpm install`
- `frontend/openapi.json` ⟵ backend sources → runs `dump-spec`
- `frontend/src/api/` ⟵ `frontend/openapi.json`, `openapi-ts.config.ts`
  → runs hey-api codegen

Key phony targets:
- `make dev` — depends on `migrate` + `generate`; then `make -j` runs
  `cargo watch` (backend `:8080`) **and** `vite` (`:5173`) in parallel.
- `make generate` — alias for the spec-dump + codegen chain.
- `make watch-api` — optional: watches backend, auto-regenerates the client.
- `make migrate` / `make prepare` — DB migrations / sqlx offline cache refresh.
- `make typecheck` — `tsc -b --noEmit`.
- `make check` — fmt + clippy + cargo-deny + typecheck + tests (the CI mirror).
- `make build` — `vite build` then `cargo build --release` (embeds the SPA).

Requires `cargo-watch` (dev only).

---

## 11. Auth (minimal)

- A single JWT **Bearer `SecurityScheme`** (poem-openapi `#[derive(SecurityScheme)]`)
  lives in `app-web`, applied to one guarded route in `modules/users`.
- The security scheme flows into the OpenAPI spec and therefore into the
  generated TypeScript client.
- No user store, no token issuance backend — just the scheme and verification.
  Everything beyond that is a documented seam.
- `JwtVerifier` has two modes, chosen from config:
  - **HS256 (dev/test)** — a shared secret (`APP_JWT_SECRET`); self-contained and
    can *issue* tokens, which examples and integration tests use.
  - **JWKS (prod)** — set `APP_JWT_ISSUER` (an OIDC issuer) and verification runs
    against the provider's published keys, discovered from
    `{issuer}/.well-known/openid-configuration`. Verify-only; keys are cached by
    `kid` and refetched on demand when a token presents an unknown one (rotation).
    Optionally set `APP_JWT_AUDIENCE` to require a matching `aud` claim.

---

## 12. Observability

- `app-telemetry` initializes `tracing-subscriber`: JSON output in prod, pretty
  in dev.
- `poem`'s `Tracing` middleware + a request-id / correlation-id middleware in
  `app-web`.
- `/health` (and readiness) endpoint in `app-web`.
- OpenTelemetry export left as a commented seam in `app-telemetry`.

---

## 13. Testing

**Backend — integration-first (the default, ~90%).**
- Tests run against the **docker-compose Postgres** (the same server as local
  dev). `make test` / `make check` depend on `db-wait`; CI starts the same
  compose service. Each test is `#[sqlx::test(migrations = false)]`, which
  creates a fresh `_sqlx_test_*` database per test on that server, hands the
  test a `PgPool` to it, and drops it when the test passes. Tests run in parallel
  with full isolation.
- `DATABASE_URL` for tests names the `postgres` database (`TEST_DATABASE_URL` in
  the Makefile), not `app`: sqlx keeps a `_sqlx_test` bookkeeping schema in the
  database it connects to. `SQLX_OFFLINE=true` stays on so `query!` compiles
  against the `.sqlx` cache, not that database.
- Each test begins by migrating (`app_db::run_migrations(&pool,
  &[module::migrations()])`) and seeding (`module::seed::standard(&pool)`).
  `migrations = false` is required: the macro's own migrator would apply
  `migrations/` into `public` with one `_sqlx_migrations` table, bypassing
  schema-per-module (§6).
- **Outside-in.** A module's behaviour is tested only through its HTTP surface,
  through every real layer. One test target per module:

  ```
  tests/<module>/
  ├── main.rs        # declares the files below
  ├── driver.rs      # the only code that knows routes, JSON, statuses, tokens
  ├── registering.rs # one file per behaviour area…
  └── …
  ```

  The **driver** boots the module's `Api` over the per-test pool (migrate, seed,
  build), exposes domain verbs (`notes.edit(FIRST_NOTE, ..)`,
  `users.signed_in_as(ALICE).me()`), and turns each response into
  `Result<View, Refusal>` (`Invalid`, `NotFound`, `Conflict`, `NotSignedIn`).
  A view wraps the returned JSON (`Deref<Target = serde_json::Value>`), so a test
  can compare it whole against a seed aggregate, or — when some properties can't
  be predicted, like a generated id or timestamp — assert on parts of it with
  `app-test-support`'s JSON path macros (strict match against the first value a
  path selects).
- **Tests speak the domain, not HTTP.** Names state a behaviour
  (`an_edit_without_a_title_is_rejected_and_changes_nothing`); bodies call driver
  verbs and compare against seed aggregates (`Ok(seed::first_note().into())`).
  One behaviour per test; a refused action is followed by a check that nothing
  changed.
- No separate service tests: every use case is reachable through the `Api`, and
  a fresh database costs the same either way. Add one only when a use case has no
  HTTP entry point (a job, a consumer) or two outcomes are indistinguishable over
  HTTP.
- No in-memory Fakes for Postgres. A hand-written **Fake** behind an interface is
  reserved for an external dependency that cannot be run locally.
- Known edges of `#[sqlx::test]`: with Postgres down, each test binary waits
  sqlx's fixed 30s pool timeout, then panics `PoolTimedOut`. Per-test database
  names derive from the test path, so two concurrent runs against one server
  collide. A failed test leaves its database behind (the next passing run of that
  test drops it); `make test-db-clean` removes all of them.

**Backend — seeding (first-class).**
- Every module owns `src/seed.rs`, compiled only with its `seed` cargo feature.
  The crate enables the feature for its own tests with a self dev-dependency
  (`users = { path = ".", features = ["seed"] }`); production builds never do.
- A seed module exposes: a **named `const` id** per well-known entity
  (`users::seed::ALICE`), a function returning that aggregate
  (`seed::alice() -> User`) for expected values, and `seed::standard(&pool)`,
  which saves the standard dataset through the real repository.
- Seeds are built with the real domain constructors — which take the id and
  timestamp as arguments — so they satisfy every invariant and there is no second
  copy of the SQL. Timestamps are fixed and spaced apart so ordering is
  deterministic.
- Drivers start from `run_migrations(..)` + `seed::standard(..)`; tests address seeded
  data by constant, never by ids captured at runtime.
- Well-known ids are readable UUIDs: `MMMMMMMM-KKKK-0000-0000-NNNNNNNNNNNN` —
  module number, entity kind within the module, sequence (e.g. Alice is
  `00000001-0001-0000-0000-000000000001`). Module numbers:

  | Module | Number | Kinds |
  |---|---|---|
  | `users` | `1` | `1` user |
  | `notes` | `2` | `1` note, `2` author |

  Seeds never reference another module's seed ids (isolation, §D1); cross-module
  scenarios choose values in the test.

**Backend — inline unit tests (the ~10%).**
- Reserved for pure, side-effect-free logic — aggregates and value objects in the
  domain layer are the natural home. Where the state space is combinatorial, use
  **`proptest`** (property-based) rather than example tables.

**Frontend — Vitest, minimal mocking.**
- Prefer rendering real components/hooks. When a test must cross the network,
  mock at the **network boundary with MSW** — never module/import mocks — so the
  real generated client stays in the path. One example test ships.

---

## 14. Production build & CI

**Dockerfile (multi-stage):**
1. `node` stage — `pnpm install` + `vite build` → `frontend/dist`.
2. `rust` stage — `cargo build --release` (embeds `dist` via rust-embed),
   `SQLX_OFFLINE=true`.
3. Runtime — `gcr.io/distroless/cc`, copies the single binary. Tiny, secure,
   glibc (avoids musl/`aws-lc-rs` toolchain friction).

**GitHub Actions `ci.yml`** mirrors `make check`:
- `cargo fmt --check`
- `cargo clippy -- -D warnings` (incl. per-crate `disallowed-types`)
- `cargo deny check`
- `cargo build` with `SQLX_OFFLINE=true`
- `docker compose up -d --wait postgres`, then backend tests with
  `DATABASE_URL` → the compose server's `postgres` database
- frontend: `pnpm install`, `make generate`, `tsc -b --noEmit`, `biome ci`,
  `vitest run`, `vite build`

**Git hooks** — `lefthook` pre-commit: rustfmt, clippy, biome, `tsc`.

---

## 15. Key decisions & rationale

Recorded because a future reader will ask "why this way?"

### D1 — Crate-per-module modular monolith
**Decision.** Each module is its own crate; layers are sub-modules within it.
**Why.** Converts architecture rules into compile errors (a module can't import
a sibling it doesn't depend on) and makes each module liftable into a
microservice. **Alternatives rejected.** Crate-per-layer-per-module (max
enforcement, 3× the Cargo churn — over-engineered for a template); single crate
with `mod` boundaries (isolation is convention-only). **Trade-off.** More crates
and a 16-element `OpenApiService` tuple cap to design around.

### D2 — `common` split by pure vs framework-coupled
**Decision.** No single `common` crate; several focused crates, split on whether
they carry a web/db dependency. **Why.** A god-`common` that pulls in poem+sqlx
would force the domain to transitively depend on both, breaking the
persistence-/framework-agnostic domain rule and the §4 bans, and would cascade
recompiles. **Trade-off.** More crates to wire; `error` (pure) and `web` (poem
mapping) are deliberately separate.

### D3 — Integration-first testing with `#[sqlx::test]` on docker-compose Postgres
**Decision.** Default to integration tests against real Postgres; inline unit
tests only for pure domain logic. Each test is `#[sqlx::test(migrations =
false)]` against the docker-compose server, which gives it a fresh database;
the test migrates with our runner and seeds. **Why.** Tests the real seams
(api→application→domain→db) with real Postgres behaviour, one server for dev,
tests, and CI, and per-test isolation at the cost of a `CREATE DATABASE` rather
than a container start (about 3.5× faster than a container per test).
**Superseded.** This originally chose a testcontainers Postgres per test and
rejected `#[sqlx::test]` as "requires an already-running Postgres and only
covers the DB". Both objections went away: `make test` and CI guarantee the
compose server, and the macro only supplies a pool — api tests still drive every
layer through poem's `TestClient`. **Alternatives rejected.** A shared
testcontainers instance feeding `#[sqlx::test]` (sqlx reads `DATABASE_URL`
before the test body, so the container must start pre-main in every test binary,
and statically-held containers are never removed); a hand-rolled per-test-DB
helper (rebuilds the macro). **Trade-off.** Tests need compose up (a clear
`make` dependency, but a bare `cargo test` without it hangs 30s per binary);
concurrent runs against one server collide; failed tests leave databases for
`make test-db-clean`.

### D4 — Embed the frontend in the binary for prod
**Decision.** `rust-embed` the built SPA into the server binary; Vite dev proxy
for local. **Why.** Single deployable artifact with no separate static host,
while keeping HMR-grade local DX. **Alternative noted.** Serve `dist/` from disk
(`StaticFilesEndpoint`, free SPA fallback) or ship to S3/CloudFront (documented
future variant).

### D5 — Handlers take the pool via `Data<PgPool>`
**Decision.** `Api` structs are unit structs; the pool is injected as a request
extension. **Why.** Makes `dump-spec` a pure, DB-free, sub-second step — the
fast inner loop depends on it. Testability comes from the `Repository` trait,
not from pool-on-struct DI.

### D6 — Application layer between api and domain
**Decision.** Four layers per module. Handlers call an application **service**;
the service loads an **aggregate** from a **repository**, runs a domain action
with a **command**, and saves the aggregate. The `Repository` trait belongs to
the application layer; the domain is pure, synchronous, and returns its own
error enum. **Why.** Built for modules that grow: business rules live on the
aggregate (one place, unit-testable without async or a database), orchestration and
cross-aggregate rules live in the service, and handlers stay translation-only.
The repository returns aggregates, not rows/DAOs, so invariants cannot be
skipped by writing fields directly. **Alternative rejected.** Free domain
functions taking `&impl Repository` (the original shape) — fine for CRUD, but
logic and I/O interleave and there is no single home for invariants. **Trade-off.**
More types per feature (command, aggregate action, service method, repo method)
and ceremony that is overkill for the example modules. Load→act→save has a
lost-update window under concurrent writes; add a version column and optimistic
check in `save` when a module needs it.

---

## 16. Open items / future variants (not built by default)
- S3/CloudFront static-hosting deploy path (alt to embedded binary).
- OpenTelemetry exporter (seam present).
- Playwright E2E.
- Kafka (or other) module external deps — add them as docker-compose services
  the same way; testcontainers remains an option for deps tests must own.
- `make seed` — a `server` bin that runs every module's `seed::standard` against
  the local docker-compose database, for a populated dev environment.
- Migrate + seed once into a template database and clone it per test, if
  per-test migration becomes the bottleneck.
- `rust_arkitect` within-crate layering tests, if any module keeps layers as
  sub-modules instead of crates.
