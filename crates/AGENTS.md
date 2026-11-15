# Backend (`crates/`)

Rules for the Rust backend, a modular monolith: each module is a crate that owns its schema and could be lifted out into its own service (ADR 0001). The repo-wide rules are in the root `AGENTS.md`. Paths are from the repo root.

**Copy the reference code.** This file holds what the code can't show you: the rules, the reasons, and what isn't built yet.

| Doing | Copy |
|---|---|
| A write module: layers, an aggregate root with children, `If-Match`, lists, events, seeds, tests | `notes` (its Note owns its Items) |
| Providing a `<module>-api` crate, an auth-guarded route, a port with an adapter | `users` (`users_api`, `/users/me`, `Mailer` in `mail/`) |
| Calling another module (decide or validate on its data) | `notes` → `users_api::UserDirectory`, passed in as `notes::module(users::directory)`, stubbed in `notes`' driver |
| Reacting to another module's events | `notes`' `application/reactions.rs`, its `handlers(..)`, its driver's `user_deleted(..)` / `run_pending_events()` |
| Keeping a copy of another module's roots, from `earliest` | none yet: `app-db`'s `tests/delivery/earliest.rs` for the semantics |
| Listing, searching or sorting by another module's columns | `note-search`, a read module over `users_api.user_v1` and `notes_api.note_v1` |
| Retries with an `Idempotency-Key` | `notes`' `behaviours/retrying.rs` and its driver's `with_idempotency_key(..)` |
| A cross-module journey | `crates/server/tests/journeys/` |
| MCP tools for an operation | nothing to write: see `app_web::mcp` and `crates/server/src/tools.rs` |

## Commands

- `make new-module NAME=<module> [ENTITY=<aggregate>]` scaffolds and registers a write module (see "Adding a module").
- `make prepare` after changing any `query!` (refreshes the checked-in `.sqlx`; needs a migrated DB). `make migrate`, `make seed`, `make check-sql`, `make check-deps`, `make check-modules`, `make check-spec`, `make db-up|db-wait|db-down`; the rest: `make help`.
- One test: `make db-wait`, then `SQLX_OFFLINE=true DATABASE_URL=postgres://app:app@localhost:5432/postgres cargo test -p users registering::` (any substring of the test path). The database is `postgres`, never `app` (tests create theirs beside it); without `SQLX_OFFLINE=true`, `query!` fails with `relation "users.account" does not exist`.
- Rust CLI tools are repo-local, run as `cargo bin <tool>` (`cargo bin cargo-deny check`), never a global `cargo deny`/`sqlx`. To bump one, edit `[workspace.metadata.bin]` in the root `Cargo.toml` with a range qualifier (`^0.20`) and re-run `make setup`.
- For Rust symbols (callers, implementations, a type) prefer the LSP tool to grep: grep misses re-exports and trait impls. Grep for strings and non-Rust files.

Gotchas:
- A test panicking `connect to DATABASE_URL`: Postgres is down (`make db-up`).
- Two test runs against one Postgres at once (two worktrees, cargo-watch beside `make test`) collide: template and per-test database names derive from the template name and test path.
- A failing test keeps its `_test_*` database to inspect until it next runs; renamed tests leave theirs behind: `make test-db-clean`.

## Architecture

`make check` enforces the module graph, schema isolation, registration and per-crate clippy bans (ADR 0017). Everything below it can't see. Breaking these defeats the template's purpose.

### Layers (ADR 0007)

Each module is a crate (`crates/modules/<module>/core`, package `<module>`) with layer folders `web/`, `application/`, `domain/`, `db/`, each re-exported from its `mod.rs` (`crate::domain::User`, never `crate::domain::user::User`). **Dependencies point inward**: `web → application → domain`, `db → application`; a use case flows handler → service → repository. Grow a layer by adding files by concern, not by growing one file.

- **`web/` translates and holds no logic.** `#[OpenApi]` on a **unit struct** `XEndpoints` taking its service as `Data<&XService>`, never struct state, so `dump-spec` builds the spec with no database (ADR 0006). Request → command → service → HTTP. Every operation returns `app_web::ApiResult<T>`, so every one declares the shared `ErrorBody` error responses; `?` converts an `AppError`.
- **`application/` owns the unit of work.** `XService` holds `Arc<dyn XRepository>` (a trait object, not a generic), `Arc<dyn EventPublisher>`, `Arc<dyn Transactions>` and `Arc<dyn Clock>`. One method per use case: load → domain action → save and publish in a unit of work (`begin` … `commit`). It mints ids, reads `self.clock.now()` (never ambient time, ADR 0020), and owns cross-aggregate rules. The repository trait lives here: `get → Option<Versioned<X>>`, `save(&mut Tx, &X, Expected) → Version`, `list(&ListQuery<XSortKey>) → Listed<Versioned<X>>`; reads run on the pool, writes take the `Tx`. `error.rs` maps the domain error onto `AppError` exhaustively, pinning it to the input at fault with `AppError::invalid_field(path, msg)` where it can (`path` as the client names it, `authorId`).
- **`domain/` is pure and synchronous**: no repositories, no async, no `Utc::now()`, no `XId::new()`, never `poem`/`sqlx`. Constructors take the id and `now`, so the same constructors build fresh aggregates and seeds; `restore(..)` rebuilds persisted state.
- **`db/` holds the only raw SQL**; a write runs on `app_db::conn(tx)`. `save` is an INSERT (`Expected::New`) or a version-guarded UPDATE (`Expected::Version`), never an upsert. Map a constraint violation by its name with `app_db::map_db_error` (copy `users`); anything else is `AppError::internal`. A crate with `migrations/` needs `build.rs` (`cargo:rerun-if-changed=migrations`), or Cargo reuses a build lacking the new one.
- **`di.rs`** builds the service in a `provider(ctx, ..)` from the shared `app_module::Context` (pool + clock), its worker handlers in a `handlers(ctx, ..)`, and bundles them with the migrations and seed in `module(..) -> app_module::Module` (ADR 0019). `lib.rs` exports only the `Endpoints`, public domain types, migrations, `di.rs` constructors, the ports a driver fakes, and `pub mod seed` under the `seed` feature (none of the last three for a module that owns no data).
- An external system other than the database is a port in `application/`, with its adapter in a folder named for it (`users`' `mail/`).
- `#![allow(clippy::disallowed_types)]` only in `db/mod.rs`, `di.rs`, `seed.rs` and test `main.rs`, never under `application/` or `domain/`.

### Aggregates and versions (ADR 0016, 0030)

- **An aggregate is a consistency boundary**, drawn around the rules that must hold in one transaction, not around tables or modules. An entity is a **child** when its root's rules span it, it can't outlive the root, and there are few enough per root to load them all (a Note's Items: unique text, at most 50); otherwise it is a root, referred to by id. Only a root has a repository, a service and a topic. A module may own several roots.
- **A use case saves exactly one root** and may read others to validate. A consequence for another root is a subscriber to the first root's event; a subscriber may change several roots in one delivery, each saved and announced as its own change.
- **The root's version is the aggregate's only one.** It lives in `app_persistence::Versioned`, never in the aggregate. Every write, a child's included, saves the root with its version guard: commit order per key, and so event order, depends on it. A child's operations take and answer with the root's `ETag` (`notes`' `/notes/:id/items`), so any change makes every client's copy stale. That is deliberate: a version per child was rejected (ADR 0030).
- Single-resource responses are `app_web::WithETag<XResponse>`, with `version: string` mirrored in the body. The expected version comes only from `If-Match` via `app_web::if_match_version` (absent → 428, stale → 412), never the body. A lost race is stale, never retried. In `db/`, a child's rows are rewritten beside the root's row in the same `Tx`, after its guard passes (`notes`' `db/items.rs`).

### Events and subscribers (ADR 0011–0015)

Built on Kafka's model: a topic per root, keyed by the root's id, ordered per key by its version, with a subscription as a consumer group, hand-rolled on a Postgres outbox.

- **Every state change publishes one event per root, in the unit of work that saves it**, so it goes out if and only if the write commits: each use case that saves a root calls `self.publisher.publish(&mut tx, events::pending(&x, version, event))` with the `Tx` it saved in. Never call an external system or another module after `commit`: anything else that must follow a write (mail, another module's reaction) is a subscriber.
- **The domain decides what happened**: each transition returns a variant of the aggregate's plain `#[must_use]` `XEvent` in `domain/` (constructors `(Self, XEvent)`, mutations `XEvent`, `Option<XEvent>` where they may change nothing, deletion `x.delete() -> XEvent`), a past-tense fact, never a command in disguise (`UserRegistered`, not `SendWelcomeEmail`). `application/events.rs` maps it exhaustively to the topic's enum, the one place domain names meet the wire.
- **Each root has a topic**, declared with `app_events::topic!` in the `-api`'s `events::v1` (copy `notes_api::events::v1`): name (`notes.note.v1`), `source`, `retention`, entity, key, one line per event (`NoteDeleted => Deleted: "notes.note_deleted"`, `NoteItemAdded => ItemAdded(OfItem): ".."` for one with a context). Its event enum is deliberately exhaustive, not `#[non_exhaustive]`: a new event breaks every whole-topic consumer in the same change. Events and the entity only gain fields, each `#[serde(default)]`; removing an event or changing the entity incompatibly is a new topic (`notes.note.v2`) published beside the old. Its module declares it publishes it (`Module::new(..).publishes::<NoteTopic>()` in `di.rs`), so `migrate` registers it.
- **The outbox is a compacted log** (ADR 0033). Each topic declares its `retention:` in `topic!`, with no default, and `migrate` registers it in `messaging.topic`; an unregistered topic is never compacted. Compaction (`Worker::compact`) deletes a relayed event once it is older than its topic's retention and a later version of its key exists: every key's latest event stays forever, a deletion's included, and jobs are copies it never touches. So a consumer can't rely on the outbox for history older than the retention.
- **An event's payload is `EventPayload { entity, context }`.**
  - **`entity` is the root's latest state** (event-carried state transfer), so subscribers never call back; a deletion carries the state as it was last. The entity is an API, not a copy of the aggregate: its type lives in the `-api`'s `events::v1`, apart from any published view record, built in `application/events.rs`. Usually that is every property but the audit ones; leave out what consumers shouldn't depend on or see, and reshape a field where the domain's own form would leak (a secret, an internal code). Each published field is coupling you must keep.
  - **`context` says what changed where the state can't.** An event about a child names it: `NoteItemRemoved` carries the Note as its entity and `OfItem { item_id }` as its context, since from the latest state alone a consumer can't tell which item went. Use none where the entity says it all (`NoteCreated`).
  - It carries the root's new version: `PendingEvent::on::<XTopic>(version, &state, event)` takes the `Version` `save` returned, or for a deletion the one removed `.next()`, so it is the newest of its key.
- **Subscribers are idempotent.** A subscriber implements `app_persistence::Handle<E>` for one event (receiving `EnvelopeOf<E>`) or `Handle<XTopic>` for a whole topic (matching its `event` with no wildcard arm) in its `application/`, registered in its `handlers(..)` under a stable queue name (`Handlers::new().on::<UserDeleted>("notes.archive_deleted_authors_notes", service)`). Handlers run in the worker inside their own `Tx`, at least once, in order per key within each queue, with no order across keys, topics or queues, and gaps where a subscription is filtered or newer than the key. A failing one backs off and holds up only its key, then is dead-lettered in `messaging.job`. A subscriber keeping a copy of another module's state stores the version and applies each event with `INSERT … ON CONFLICT (id) DO UPDATE … WHERE copy.version < EXCLUDED.version`, with a tombstone for a deletion, since a rerun dead letter arrives out of order; reactions to facts (mail, archiving) don't compare versions.
- **Subscriptions are rows in `messaging.subscription`, registered by `migrate`**, never on worker start, so old and new builds relay alike during a rolling deploy (ADR 0014). A queue is bound to one topic forever and a retired name is never reused. Retire one with a migration, never by just deleting its handler:

  ```sql
  UPDATE messaging.subscription SET retired_at = now() WHERE queue = '<queue>';
  UPDATE messaging.job SET dead_at = now(), last_error = 'subscription retired'
  WHERE queue = '<queue>' AND dead_at IS NULL;
  ```
- **A subscription starts from `latest`** (`Handlers::on`): it receives the events relayed after `migrate` first registers it. **A copy of state starts from `earliest`** (`Handlers::on_from_earliest`), subscribing to the whole topic and comparing versions: `migrate` backfills it once, at its first registration, with every relayed event its topic's outbox holds (after compaction, each key's latest plus the history within the retention). A key's live events wait for its backfilled ones, but other keys' don't, so it can't assume it has seen all history before its first live event. A reaction to facts starts from `latest`, or it would re-run for history. A start is fixed once registered: changing it in code only makes `migrate` warn.
- **There are no jobs or scheduled work** (ADR 0015). Don't add them until a module needs "do X at time T". Compaction is no exception: it is the worker's own maintenance of the log, not module work. Each worker compacts after its first poll, then after its first poll once `APP_OUTBOX_COMPACT_EVERY` has passed by its clock (`Worker::compact_if_due`), uncoordinated: N replicas compact up to N times a period, which the database alone makes safe, so add no lock, lease or schedule table.

### Lists (ADR 0018)

Take `ListQuery<XSortKey>` and return `Listed<T>`; build the SQL with `QueryBuilder` + `app_db::push_list_clauses` (the one sanctioned runtime SQL, since `query!` can't vary `ORDER BY`), ending in a unique tiebreak. The handler takes separate `limit`/`offset`/`sort` params and returns `ListResult<XResponse>`. Copy `notes`.

### Across modules (ADR 0002, 0008–0010)

- **Choose the mechanism by why the data is needed, not how much.** To decide or validate, call the provider's `<module>-api` trait at any size (a bulk need gets a batch method like `users_exist(ids)`). To list, search, filter, sort or page by another module's columns, write a **read module** joining the providers' published views. A write module never reads views or joins across schemas.
- **Modules meet only through pure `<module>-api` crates** (no poem/sqlx), never a core crate. Ids cross as plain `Uuid`; each side keeps its own typed ids and converts in `application/`. The provider implements the trait in `application/` and exposes a constructor from `di.rs`; `server` passes it to the consumer's `module(..)`. Never write another module's data, through its `-api` or otherwise: publish, and let the owner react.
- **No foreign keys between modules**: a cross-module id is a plain `uuid` column, validated on write through the provider's trait where the rules need it; consumers tolerate it dangling. A foreign key would couple migration order, deletes and locks, and stop a module being lifted out.
- **Read modules** own no source data and have no `domain/`: an `XFinder` trait returns read models. A provider publishes views in its own `<schema>_api` schema (`users_api.user_v1`) from its own migrations; a view only gains columns, a breaking change ships as `_v2` beside `_v1`. Each view has a **published record** in the provider's `-api` (`users_api::v1::User`), written with its version path, `#[non_exhaustive]`, gaining a field when the view gains a column, and never returned from a trait. A read module reads others' data only through views, even for one row, declares each in `[package.metadata.module] reads` (`make check-sql` enforces it), never calls `-api` traits, has no seed, and depends on provider cores only as a test-only dev-dependency for their `migrations()` and `seed::standard`.
- **Isolation** is enforced three ways (ADR 0017); keep all passing: the Cargo graph (`make check-deps`: no core depends on another core, an `-api` depends only on pure crates, a write module declares no `reads`); `deny.toml` (every module core, read modules included, in both the `sqlx` and `poem` `wrappers` lists, never an `-api`); a per-crate `clippy.toml` (copy a sibling's) under `-D warnings`.

### Common crates (ADR 0003)

Split pure (`app-domain`, `app-error`, `app-events`, `app-persistence`, `app-test-support`) from framework-coupled (`app-web` = poem, `app-db` = sqlx, `app-module` = both, for `di.rs` and the server; `app-test-db` = sqlx, tests only). Never merge them, and never add poem/sqlx to a pure crate. Declare ids with `app_domain::uuid_newtype!`. Config and tracing live in `crates/server`, not `common`.

### API surface

- **One error shape** (ADR 0022): every failure is an `ErrorBody` (`code`, `message`, optional `fields: [{path, message}]`, `requestId`). Handlers return `ApiError`, and `render_error` (applied with `catch_all_error`) renders framework failures the same way. Clients branch on `code` (`validation_error` vs `malformed_request` etc.), never on `message`. `RequestIdMiddleware` is outermost, so every response carries `x-request-id`, and the body echoes it.
- **`Idempotency-Key`** (ADR 0032) is shared middleware (`app_web::Idempotency`, built by `Context::idempotency()`): any POST, PUT, PATCH or DELETE gets it with no code. The first request with a key runs; its 2xx answer is replayed for 24 hours to a retry with the same key and request (method, target, `If-Match`, body); the same key with another request is `idempotency_key_reused` (422), one still running `idempotency_key_in_flight` (409); a failure keeps nothing. Keys are the caller's (the bearer token's subject, else one anonymous scope). `server::build_service` wraps the `Endpoints` in `app_web::Idempotent`, which documents the header and refusals on every unsafe operation.
- Request/response types expose ids, timestamps and versions as `string`. Past 12 modules, nest sub-tuples in the `Endpoints` tuple (std's `Default` cap; poem-openapi's is 16).
- **Every operation is an MCP tool** (ADR 0034), served at `/mcp` by `app_web::mcp` with no code per operation: listed from the spec, each call sent in-process through the same endpoint `/api` is served by, so auth, `If-Match` (the tool's `expected_version`), `Idempotency-Key` (its `idempotency_key`), validation and the `ErrorBody` behave as over REST. So every operation declares `operation_id` (snake_case, verb first, unique: `edit_note`), the tool's name and a public contract, and a summary written for an agent as much as for a person; `composed_app.rs` fails on one missing either and lists every tool, so the surface shows in a diff. Keep an operation off MCP only through `REST_ONLY` in `crates/server/src/tools.rs`, with its reason; one whose body isn't JSON must be listed there. Every `/mcp` request needs a valid bearer token of the subject who opened its session. A new refusal status needs its line in the instructions there (a composed test fails otherwise).
- **Auth** (ADR 0025): one JWT Bearer scheme (`app_web::JwtAuth`) guarding one route. Don't build a user store or token issuance unless asked.
- **Pinned** (ADR 0026): `poem` `3.1.x` ↔ `poem-openapi` `5.1.x` (released in lockstep); MSRV 1.94, set by `sqlx` 0.9.

### Running and deploying

- **Migrations run explicitly** (`make migrate`, a pre-start task in a deployment), never on server startup (ADR 0021). Each module migrates its own schema. A migration file applied anywhere is immutable, its comments included (sqlx checksums it). `SQLX_OFFLINE=true` is the default; regenerate `.sqlx` with `make prepare`.
- **MCP sessions live in the process that opened them** (ADR 0034): behind several server instances, the load balancer routes by the `Mcp-Session-Id` header. `APP_PUBLIC_URL` sets the URL MCP clients are told to sign in for; unset, each request's `Host` and `X-Forwarded-*` say.
- **One image, three processes**: `server`, `worker` (its own deployment, scaled by replicas sharing the queue) and the one-off `migrate` (and `seed`, built with `SEED=true`, which refuses `APP_ENV=prod`). A server built without `embed-frontend` answers `/` with a page pointing at Vite.

## Tests (ADR 0004, 0029, 0031)

Tests are **outside-in**: a module is tested only through its `Endpoints`, through every real layer, against real Postgres. One target per module, named after it: `tests/<module>/` with `main.rs`, `driver.rs`, and `behaviours/` holding one file per behaviour area. Copy `notes`' `tests/notes/`.

- **Each DB test is `#[app_test_db::test(crate::driver::TEMPLATE)] async fn name(pool: PgPool)`**, never `#[sqlx::test]`. The driver's `TEMPLATE` (`Template::new("<module>", |pool| ..)`, its name unique across test binaries) migrates and seeds once per test binary, and each test gets a copy of it with a pool of its own. `sqlx::test` migrated and seeded every test from empty and shared one 20-connection pool among a binary's tests, so tests holding several connections timed out. Anything a test changes per run (clock, subscriptions) stays in the driver's boot, not the template.
- **The driver alone knows HTTP.** It boots via `X::seeded(pool)` (a `Context` with a `FixedClock` after the seeds, its worker from `handlers(&ctx, ..)` registered with `app_db::register_subscriptions`), exposes domain verbs, and returns `Result<XJson, Refusal>` with only the variants the module produces, mapped in `From<Refused>` from the error `code`, not the status. The transport is `app_web::testing` (the `test` feature, dev-dependencies only): `TestApi::new(XEndpoints, ctx.idempotency(), wire)` mounts them as the server does, and `send()` checks every refusal has the error shape and echoes the request id; `answered()` returns the whole answer, for comparing a retry with the original. Response bodies are `json_newtype!(pub XJson)` with `From<Aggregate>`, lists `Page<XJson>`.
- **Tests speak the domain**: a name states one behaviour (`an_edit_without_a_title_is_rejected_and_changes_nothing`); bodies never mention routes, methods or status codes. Compare whole against seeds (`Ok(seed::alices_note().into())`), or use `assert_json_matches!` / `assert_json_exists!` / `assert_json_not_exists!` for unpredictable or must-not-leak fields. A refused action is checked to have changed nothing and announced nothing; a state change, to have announced its event at its version (`announced::<E>()`).
- Background work runs only when a verb runs it (`run_pending_events()`); time moves only via `later(by)`. A subscriber's driver publishes the provider's event itself (`notes.user_deleted(USER_ID_ALICE)`), as it stubs an `-api` trait.
- **No stand-ins for Postgres.** The only stand-ins are a consumer's driver stubbing an `-api` trait, and a hand-written Fake for an external dependency that can't run locally (`users`' `FakeMailer`). No service tests unless a use case has neither an HTTP entry point nor a worker verb. Inline tests only for pure domain logic (`proptest` when the state space is combinatorial), and for shared plumbing a driver can't provoke (`Handlers`).
- **Journeys** exist because each module's tests stub what's across their boundaries, so none notices a payload drifting from what its subscriber expects or the server's wiring breaking. A journey walks one path across modules on the composed app (every registered module, the real worker, real JWT, nothing stubbed), asserting every step and re-testing no rules: `crates/server/tests/journeys/` (one file per journey area, driver `tests/driver.rs`, `App::seeded(pool)`), run with `cargo test -p server journeys::`. `server` sets `autotests = false`, so a new target there goes in its `Cargo.toml`. The composed-app test (`tests/composed_app.rs`) fails when a registered module's provider or migrations don't reach the server.

**Seeds** (ADR 0028). Every module that owns data has `src/seed.rs` behind its `seed` feature (enabled for its own tests by a self dev-dependency): `const` ids, one `fn` per aggregate, and `seed::standard(&pool)`, which saves what's missing. Build seeds with the real constructors at fixed `app_domain::seed_time(minutes)`, with ids from `app_domain::well_known_id(MODULE, kind, sequence)`, where `pub const MODULE: u32` is unique across modules. Name each id constant `<OWNER>_<CLASS>_<IDENTIFIER>` (`USER_ID_ALICE`, `ALICE_NOTE_ID_SHOPPING_LIST`, `SHOPPING_LIST_ITEM_ID_MILK`), so the class never reads as part of the identifier. Refer to seeded data by constant, never captured ids. A module with an `-api` publishes its seed there under the `-api`'s `seed` feature (`MODULE`, the ids as `Uuid` consts, one `fn` per entity returning its published record); a seed or test names another module's seed data only through its `-api`. A module seeds only its own data.

## Adding a module

`make new-module NAME=widgets` (Postgres up) copies `crates/module-template/{core,api}` (workspace crates, packages `widgets` and `widgets-api`, compiled and tested but registered with no server) into a module that compiles, passes its tests and is registered everywhere: every layer, a versioned aggregate with create/read/rename/list publishing its events, migration, seed and tests. It allocates the next module number, applies the migrations to the dev DB and refreshes `.sqlx`. Then:

1. Reshape the aggregate, keeping its migration, seed and tests in step; `make prepare` after changing a `query!`.
2. If it uses another module, depend on that module's `-api` and pass the provider's constructor to its `module(..)` in `crates/server/src/modules.rs`, after the module it uses.
3. `make generate`; for a UI, follow `frontend/AGENTS.md`.
4. If it reacts to, provides for, or is read by another module, add or extend a journey.
5. Done when `make check` passes.

A read module isn't scaffolded: copy `note-search`. Registered by hand or not, a module is listed in `crates/server/src/modules.rs` (`Endpoints` tuple and `modules()`), in `crates/server/Cargo.toml` (dependency, and `seed` feature if it has a seed), in the root `Cargo.toml` workspace dependencies (core and `-api`) and in both `deny.toml` lists; `make check-modules` fails on any missing entry.

## Not built yet

Deliberate gaps: build one only when a module needs it.

- Scheduled and recurring module work (a `messaging.job` row with a future `run_at` already waits its turn; the worker's own compaction is the only schedule).
- Rewinding an existing subscription to `earliest` or a point in time, and an admin surface for dead letters (today: SQL on `messaging.job`).
- A relay to an external broker for a lifted-out module; read modules owning event-fed projections once a provider is lifted out.
- Per-module Postgres roles with `SELECT` only on `_api` schemas, the database-enforced version of `make check-sql`.
- Search indexes for read modules (`pg_trgm`, or a `tsvector` published through the view).
- Waking the worker other than by polling.
- Ending long-lived responses (streams) on shutdown: nothing streams yet.
- An OpenTelemetry exporter (a seam in `server::telemetry`), Playwright end-to-end tests, and static hosting for the SPA apart from the binary.
- More external dependencies as docker-compose services, or testcontainers for those a test must own.
