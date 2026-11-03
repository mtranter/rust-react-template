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
| Routing | `@tanstack/react-router` (code-based routes) | `1` |
| UI components | shadcn/ui (Base UI primitives, Tailwind v4, `base-nova`) + TanStack Table | table `9` |
| Package manager | `pnpm` | — |
| FE lint/format | Biome | latest |
| FE tests | Vitest + React Testing Library + MSW | latest |
| Task runner | GNU `make` | — |
| Arch enforcement | Cargo graph + `cargo-deny` + per-crate `clippy.toml` | `cargo-deny` `0.20` |
| Rust CLI tools | `cargo-run-bin` (repo-local `./.bin`, see §10) | pinned in `Cargo.toml` |
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
├── Cargo.toml                # workspace: members = crates/common/*, crates/modules/*/*, crates/server
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
│   │   │   ├── api/          # pkg: users-api — pure: what other modules may use (D7)
│   │   │   │   └── src/          # UserDirectory trait, v1 published records, events::v1, seed (§13)
│   │   │   └── core/         # pkg: users — the module itself
│   │   │       ├── Cargo.toml
│   │   │       ├── build.rs      # rerun on migrations/ changes (§6)
│   │   │       ├── clippy.toml   # disallowed-types: sqlx::* banned outside db
│   │   │       ├── migrations/   # module-owned schema (schema `users`, views in `users_api`)
│   │   │       ├── tests/users/  # the module's one test target (§13)
│   │   │       └── src/
│   │   │           ├── lib.rs    # re-exports Endpoints, domain types, migrations, the ports a driver fakes
│   │   │           ├── di.rs     # composition root: provider(ctx), directory(ctx), handlers(ctx, mailer), module()
│   │   │           ├── seed.rs   # `seed` feature only: saves the dataset `users-api` describes (§13)
│   │   │           ├── web/          # web (HTTP) layer
│   │   │           │   ├── mod.rs        # re-exports the Endpoints
│   │   │           │   ├── handlers.rs   # #[OpenApi] impl (unit struct) → commands
│   │   │           │   └── models.rs     # request/response Objects
│   │   │           ├── application/  # application layer
│   │   │           │   ├── mod.rs        # re-exports service + repo trait
│   │   │           │   ├── service.rs    # use cases: load → domain action → save
│   │   │           │   ├── repository.rs # Repository trait (aggregates in/out; writes take &mut Tx)
│   │   │           │   ├── directory.rs  # impl users_api::UserDirectory
│   │   │           │   ├── mailer.rs     # Mailer port (an external system; Faked in tests)
│   │   │           │   ├── welcome.rs    # subscribes to its own UserRegistered
│   │   │           │   └── error.rs      # domain error → AppError
│   │   │           ├── domain/       # domain layer (pure, sync)
│   │   │           │   ├── mod.rs        # re-exports the public domain surface
│   │   │           │   ├── user.rs       # the aggregate + domain actions
│   │   │           │   ├── email.rs      # value object
│   │   │           │   ├── commands.rs   # intents accepted by domain actions
│   │   │           │   ├── error.rs      # domain error enum
│   │   │           │   └── ids.rs        # newtype ids via app_domain::uuid_newtype!
│   │   │           ├── db/           # DB layer
│   │   │           │   ├── mod.rs        # re-exports repo impl + migrations
│   │   │           │   ├── migrations.rs # SCHEMA + embedded MIGRATOR
│   │   │           │   └── repository.rs # sqlx Repository impl (insert or version-guarded update)
│   │   │           └── mail/         # the Mailer adapter the server uses (only logs)
│   │   ├── notes/            # the reference write module
│   │   │   ├── api/          # pkg: notes-api — v1 published records + seed, no traits yet
│   │   │   └── core/         # same shape, schema `notes`; uses users only via `users-api`,
│   │   │                     #   reacts to its UserDeleted (application/reactions.rs)
│   │   └── note-search/      # read module (D8): joins published views, owns no data
│   │       └── core/         # web/ + application/ + db/, no domain/ and no migrations/
│   │
│   ├── module-template/      # what `make new-module` copies; registered with no server
│   │   ├── core/             # pkg: widgets
│   │   └── api/              # pkg: widgets-api — events::v1
│   │
│   ├── common/
│   │   ├── domain/           # pkg: app-domain      — pure (uuid_newtype! macro)
│   │   ├── error/            # pkg: app-error       — pure
│   │   ├── events/           # pkg: app-events      — pure (Event, EventEnvelope)
│   │   ├── persistence/      # pkg: app-persistence — pure (versions, list queries)
│   │   ├── web/              # pkg: app-web         — poem-coupled
│   │   ├── db/               # pkg: app-db          — sqlx-coupled; migrations/ = the `messaging` schema
│   │   ├── module/           # pkg: app-module      — poem + app-db: Context and a module's registration (§4)
│   │   └── test-support/     # pkg: app-test-support — dev-only, pure test utilities
│   │
│   └── server/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs        # build_service() + module_migrations(), shared by the bins
│           ├── modules.rs    # the module registry: the Endpoints tuple + modules() (§4)
│           ├── app.rs        # build_app(): routes, middleware, every module's request data, embedded FE
│           ├── main.rs       # config + JWT verifier, then serve build_app()
│           ├── config.rs     # figment-layered env/file → typed AppConfig
│           ├── telemetry.rs  # tracing-subscriber init (JSON prod / pretty dev), OTEL seam
│           ├── shutdown.rs   # SIGTERM / Ctrl-C, shared by server and worker
│           └── bin/
│               ├── dump-spec.rs  # print OpenAPI JSON to stdout (DB-free)
│               ├── migrate.rs    # run all module migrations in order
│               ├── worker.rs     # deliver events to every module's subscribers (§4)
│               └── seed.rs       # `seed` feature only: every registered module's seed
│       └── tests/
│           ├── composed_app.rs # boots build_app() over every module (§4)
│           ├── journeys.rs     # the journeys target: mod driver; mod journeys { … } (§13)
│           ├── driver.rs       # its driver: the composed app's cross-module verbs
│           └── journeys/       # one file per journey area
│
└── frontend/
    ├── package.json
    ├── pnpm-lock.yaml
    ├── vite.config.ts        # dev proxy /api -> :8080
    ├── vitest.setup.ts       # MSW server; absolute baseUrl for the client under Node
    ├── openapi-ts.config.ts  # hey-api codegen config
    ├── components.json       # shadcn/ui config
    ├── biome.json
    ├── tsconfig.json (+ app/node references)
    ├── openapi.json          # spec snapshot (generated; input to codegen)
    └── src/
        ├── api/              # generated client (never hand-edited)
        ├── features/<module>/ # one per backend module (§8)
        ├── pages/            # thin route targets
        ├── components/       # shared: ui/ (vendored shadcn), data-table/
        ├── hooks/, lib/      # shared
        ├── router.tsx        # TanStack Router route tree
        └── main.tsx
```

**Naming**: crate *folders* under `common/` are short (`web`, `db`…) but
package **names** are prefixed `app-*` to avoid crates.io collisions and read
clearly at the `use` site.

---

## 4. Modules & layers

Each module is one crate exposing four internal layers as sub-module
**folders**. Each layer splits its contents across files by concern and
re-exports them from its `mod.rs`, so the rest of the crate addresses a layer
(`crate::domain::User`), never a file inside it. Growing a layer means adding
files (or nested folders, e.g. `web/handlers/` per resource), not growing one.

Dependencies point inward — `web → application → domain`, with `db` implementing
the application's `Repository` — and every use case flows **handler → service →
repository** (see D6):

```
handler:  request (+ If-Match) ──► command ──► service.method(command, expected)
service:  repo.get(id) ──► aggregate.action(command)? ──► tx = begin ──► repo.save(&mut tx, &aggregate, Expected::Version(v)) ──► tx.commit
```

- **`web/`** — the HTTP layer. `handlers.rs`: a `#[OpenApi]` impl on a **unit
  struct** named `XEndpoints` (no fields). Operations obtain the service via
  `poem::web::Data<&XService>`, not via struct state, build a **command** from the request, call the service, and map
  the result (an `AppResult`) to HTTP via `app-web`. No business logic.
  `models.rs`: request/response `Object`s and their conversions from aggregates.
- **`application/`** — `service.rs`: one service per aggregate
  (`UserService`, holding an `Arc<dyn UserRepository>` rather than being generic
  over it, plus an `Arc<dyn EventPublisher>`, an `Arc<dyn Transactions>` and an
  `Arc<dyn Clock>`), one method
  per use case. A service loads an
  aggregate from the repository, runs a domain action on it, and saves it in a
  unit of work (below). It
  owns what a single aggregate cannot: rules spanning many aggregates (e.g. email
  uniqueness), not-found semantics, fresh ids, and the time (`self.clock.now()`).
  `repository.rs`: the
  module's **`Repository`** trait — it speaks in aggregates, never rows:
  `get → Option<Versioned<X>>`, `save(&mut Tx, &X, Expected) → Version`, and
  `list(&ListQuery<XSortKey>) → Listed<Versioned<X>>` where the module lists.
  Reads run on the pool; every write takes the caller's `&mut Tx`. Every write
  to an aggregate, a change to a child included, saves the root with its
  version guard: the order of a root's events depends on it (below), so a
  module that writes a child row without bumping its root loses it.
  `error.rs`: an exhaustive
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
  (`app-domain`, `app-error`, `app-events`, `app-persistence`, `chrono`, `uuid`, `thiserror`) — never on `poem`
  or `sqlx`.
- **`db/`** — `repository.rs`: the `sqlx` implementation of the `Repository`
  trait, the only place raw SQL lives; `save` is an `INSERT` for
  `Expected::New` and a version-guarded `UPDATE` for `Expected::Version`
  (below). `migrations.rs`: the module's schema name and embedded migrator. Owns
  the module's `migrations/` (its own schema).
- **`di.rs`** — the composition root: `provider(ctx, ..)` returns a type
  implementing `app_web::Provide`, which builds the service over the real
  repository from the shared `app_module::Context` (the pool and the clock, made
  once by the server); the server mounts it with `.accept(..)` so each module wires its
  own request data. A module offering a `<module>-api` trait also exposes its
  implementation's constructor here (`users::directory(ctx)`), and a module with
  subscriptions a `handlers(ctx, ..)` (below). `module()`
  returns the module's `app_module::Module` (below).
- **Outbound adapters** — a port the application defines for an external system
  other than the database (`users`' `Mailer`) gets its real adapter in a folder
  named for that system (`mail/`), as `db/` is for the repository.

`lib.rs` keeps every layer private and re-exports the module's `Endpoints`, its
public domain types, its migration bundle and its `di.rs` constructors (plus
`pub mod seed` under the `seed` feature); nothing else is visible outside
(services and repositories are internal — tests go through the `Endpoints`, §13).

### Registering a module

Each module is registered in one place, `crates/server/src/modules.rs`: its
`Endpoints` in the `Endpoints` tuple (the spec's type and, via `Default`, its
value) and its `module(..)` in `modules()`. A `Module` bundles the provider,
its worker handlers (`with_handlers`), the migrations and, under the `seed`
feature, the seed (neither of the last two for a module
that owns no data, like `note-search`); the server derives everything else from that one ordered list —
`module_migrations()`, the request data `build_app()` adds, the handlers
`server::worker()` runs, and what the `seed`
bin runs. A module that uses another's `<module>-api` takes the provider's
constructor as an argument (`notes::module(users::directory)`).

Each module's own tests wire it themselves, so two checks cover the server:
`crates/server/tests/composed_app.rs` migrates and seeds through the registry,
builds the real app and requests every `GET` in the spec, failing on any `5xx`
(a missing provider or migration), and requires a probed `GET` for each tag;
`scripts/check-modules.sh` (`make check-modules`, in `make check` and CI) fails
on a module core crate missing from `modules.rs`, the server's dependencies,
its `seed` feature (for a module with a seed), or either `deny.toml` list, and
on a missing or duplicated module number (§13).

`make new-module NAME=widgets [ENTITY=widget]` (`scripts/new-module.sh`)
scaffolds a write module, its `core` and `api` crates, from
`crates/module-template/` — every layer, a versioned aggregate with
create/read/rename/list publishing `WidgetCreated` / `WidgetRenamed`, `clippy.toml`, `build.rs`, a
migration, a seed and a test target — allocates the next module number,
patches all of the above, applies its migrations to the dev database and
refreshes `.sqlx`, so its tests pass as generated. The template is itself two
workspace crates (packages `widgets` and `widgets-api`; `widgets` is the token the generator renames): it is
compiled, linted and tested with everything else and held to `check-deps` and
`check-sql`, but registered with no server, so `check-modules` skips it. Its
migrations reach the dev database only for `make prepare`
(`scripts/migrate-schema.sh`), so `.sqlx` covers its queries. CI still scaffolds
a module (`gadgets`) to test the generator itself. Read modules are rare enough
to copy `note-search` by hand.

Only `db/mod.rs`, `di.rs`, `seed.rs` and test targets carry
`#![allow(clippy::disallowed_types)]` (they wire or receive the pool); never
`application/` or `domain/`.

### Optimistic concurrency

Load → act → save has a lost-update window, so every write module versions its
aggregates. The version is persistence state, not domain state:
`app-persistence` defines `Version`, `Versioned<T> { entity, version }` and
`Expected::{New, Version(v)}`, and the aggregate never sees them.

- **db**: the table has `version bigint`. `save(&x, Expected::New)` inserts at
  `Version::FIRST`; `save(&x, Expected::Version(v))` runs
  `UPDATE … SET …, version = version + 1 WHERE id = $1 AND version = v RETURNING version`
  and maps no row to `AppError::PreconditionFailed`. The check is atomic in
  that statement.
- **application**: an edit takes the caller's `expected: Version`, loads the
  aggregate first (so a missing one is not-found rather than stale), runs the
  action and saves with `Expected::Version(expected)`; it returns the
  `Versioned<X>`.
- **web**: single-resource responses are `app_web::WithETag<XResponse>` (strong
  quoted `ETag`), and the body mirrors the version as a `version: string` field
  so the generated client can read it. Edits take
  `#[oai(name = "If-Match")] Header<Option<String>>` and parse it with
  `app_web::if_match_version` — the only source of the expected version, never
  the request body. Absent is `428 Precondition Required`; stale is
  `412 Precondition Failed`.
- **tests**: the driver's edit verb takes the version; `Refusal::PreconditionRequired`
  and `Refusal::Stale` cover the two refusals.

`notes` is the reference (`application/service.rs`, `db/repository.rs`,
`web/handlers.rs`, `tests/notes/behaviours/editing.rs`).

### Side effects and reactions: units of work and events

A use case that writes does so in one **unit of work**: `app_persistence::Tx`,
begun from the service's `Arc<dyn Transactions>` and committed at the end.
Everything written through it commits together or not at all; dropping it
uncommitted (as `?` does) rolls it back. It is opaque above `db/`: repositories
reach the connection with `app_db::conn(tx)`, so `application/` still never
names the driver. The `Tx` is only a transaction; publishing is the service's
own port, `Arc<dyn EventPublisher>`, which takes the `Tx` like a repository
write does.

```
let changed = a.change(..)?;
let (b, created) = B::create(..)?;
let mut tx = self.transactions.begin().await?;
self.repo.save(&mut tx, &a, Expected::Version(v)).await?;
self.repo.save(&mut tx, &b, Expected::New).await?;
self.publisher.publish(&mut tx, events::pending_a(&a, changed)).await?;
self.publisher.publish(&mut tx, events::pending_b(&b, created)).await?;
tx.commit().await?;
```

Anything that must follow a write goes through the same `Tx`, so it happens if
and only if the write commits:

- **The domain decides what happened.** Each aggregate has a plain
  `#[must_use]` enum of what can happen to it in its `domain/`
  (`users::domain::UserEvent { Registered, DisplayNameChanged, Deleted }`),
  knowing nothing of `app_events`. Every transition returns one: a constructor
  `Result<(Self, XEvent), _>`, a mutation `Result<XEvent, _>` (`Option` where it
  may change nothing: `notes`' `archive` of an archived note), and a deletion `XEvent` from the aggregate as it was last.
  A variant carries only what the state can't say (the event's context), never
  the state. `application/events.rs` maps each variant, exhaustively, to its
  topic's event enum in the `-api`: `pending(&aggregate, event) -> PendingEvent`,
  `PendingEvent::on::<UserTopic>(&state, event)` with the state built from the
  aggregate as it now is. That match is the one place domain names meet the
  published contract, so the domain can rename or split its events without
  touching `events::v1`; the path from domain to wire has two exhaustive
  matches, one on each side.
- **An event** (`publisher.publish`) tells other modules something happened to an
  aggregate: a fact in the past tense (`UserRegistered`), never a command in
  disguise (`SendWelcomeEmail`). **Every state change publishes one**, in the
  unit of work that saves it, one per aggregate changed (`users`: registered,
  display name changed, deleted; `notes`: created, edited, archived, deleted). The
  publisher doesn't know who listens.
- **Each aggregate root has a topic** (as Kafka has topics): one stream of its
  events, keyed by the root's id. A module's `-api` declares it with
  `app_events::topic!` (`users_api::events::v1`):

  ```rust
  app_events::topic! {
      pub topic UserTopic("users.user.v1") {
          source: "/users",
          entity: User,
          key: |user| user.id.to_string(),
          events: UserEvent, UserEventKind;
          UserRegistered => Registered: "users.user_registered",
          UserDisplayNameChanged => DisplayNameChanged: "users.user_display_name_changed",
          UserDeleted => Deleted: "users.user_deleted",
      }
  }
  ```

  It generates the `app_events::Topic` (`NAME`, `SOURCE`, `Entity`, `Event`,
  `key`), the topic's event enum with one variant per event holding its
  context (`ItemRemovedFromList => ItemRemoved(ItemRemoved): "lists.item_removed"`),
  its `decode` and `encode`, a fieldless `UserEventKind` holding each type
  string, and a marker type per event (`UserDeleted`) implementing
  `app_events::Event` (`TYPE`, `Topic`, `Context`). It fails to compile if two
  events of a topic share a type. The enum is deliberately **not**
  `#[non_exhaustive]`: a new event breaks every whole-topic consumer's build,
  in the same change. **Versioning is the topic's**: event types are
  `<module>.<event>`, unique within the topic; contexts and the entity only gain
  fields, each `#[serde(default)]`, and adding an event is additive on the
  wire; removing one or changing the entity incompatibly is a new topic
  (`users.user.v2`), published beside v1 until its consumers have moved.
- **What an event carries is an `app_events::EventPayload { entity, context }`**
  (event-carried state transfer, so a subscriber never calls back).
  `entity` is the aggregate root's whole state as it now is, every property but
  the audit ones (created/updated at/by): one plain, `Serialize` state type per
  root in `events::v1` (`users_api::events::v1::User`), shared by all that
  root's events and built from the aggregate in `application/events.rs`. It is
  kept apart from the published view record (`users_api::v1::User`, D8): today
  they hold the same fields, but a view column and an event version change on
  their own schedules. A deletion carries the state as it was last. More fields
  are more coupling, which the template accepts in exchange for consumers, not
  the publisher, deciding what they need. `context` says what happened where
  the state alone can't: an event about a child of the root (`ToDoItemRemoved`)
  carries the root (the list) as its entity and the child's id as its context
  (`ItemRemoved { item_id }`), held by its variant. Each event type fixes its
  context; `NoContext`, left out of the JSON, where the entity says it all
  (every event in this repo today: none of its aggregates has children).
- **Every event travels in an `app_events::EventEnvelope`**, a CloudEvents
  1.0 envelope in its JSON format (`specversion`, `id`, `source`, `type`: the
  event type, `subject`: the key, `time`, `datacontenttype`, `data`: the
  payload `{ entity, context? }`), with extension attributes `topic`,
  `partitionkey` (the key, from CloudEvents' partitioning extension) and
  `entityversion`;
  `EnvelopeOf<E>` names it for an event. A service hands the publisher a
  `PendingEvent`, its topic erased so every module's events share one type,
  and the publisher stamps the id and time and stores the whole envelope as
  the outbox `payload`; subscribers receive it, so they have the event's id to
  deduplicate on. `time` is when the publishing transaction recorded it, by
  that instance's clock, before it committed: it doesn't order events.
  Decoding takes two steps, since `type` and `context` sit at
  different depths, beyond a serde-tagged enum: the envelope with its context
  as raw JSON, then `UserEvent::decode(type, context)`.
- **Every event carries its root's version**, required by
  `PendingEvent::on::<T>(version, &state, event)`: on the outbox row
  (`topic`, `key`, `version`, unique together, so a second event for one save
  fails it) and in the envelope as `entityversion`. A save publishes the
  `Version` its `save` returned; a deletion publishes the version it removed
  plus one (`Version::next()`, `users`' `delete` announces `expected.next()`),
  so it is strictly the newest event of its key rather than tying with the
  last update. `entityversion` is a custom extension attribute holding an
  integer, not CloudEvents' `sequence`, which is a string that must sort
  lexicographically and has no stated scope; this one is an integer scoped to
  one `topic` and `partitionkey`.
- **A subscriber** is how a module reacts: an `app_persistence::Handle<S>`
  impl in its `application/`, registered under a stable queue name in its
  `handlers(..)`. It subscribes either to one event,
  `Handle<UserDeleted>` receiving an `EnvelopeOf<UserDeleted>` (`notes`'
  `application/reactions.rs`, deleting a user archives their notes;
  `Handlers::new().on::<UserDeleted>("notes.archive_deleted_authors_notes", service)`),
  or to a whole topic, `Handle<UserTopic>` receiving a
  `TopicEnvelope<UserTopic> { id, key, time, entity, event }` and matching
  `event` with no wildcard arm (`.on::<UserTopic>(queue, handler)`; one
  subscription row per event type). A queue is of one topic; it may list
  several of its types. A delivery of a topic or type its queue didn't
  register is refused.
  It must tolerate being run twice (and, rarely, out of order: below). A module may subscribe to
  its own events: `users` welcomes a registered user by reacting to its
  `UserRegistered` (`application/welcome.rs`), rather than the registration
  ordering the email. A side effect on an external system sits behind a port
  (`Mailer`) whose adapter lives outside `application/`.

There are no jobs or scheduled work: a module that has to do something does it
in reaction to an event, its own or another module's. Scheduling ("do X at T")
waits for a module that needs it (§16).

Events are delivered by the **worker** (`app_db::Worker`, run by `server`'s
`worker` bin). `app_db::PgEventPublisher` writes each event to the shared
`messaging.outbox` as it is published, on the `Tx` the aggregates are saved in. The worker's relay copies each outbox event
into one delivery per subscriber, a row of `messaging.job`; then each due
delivery is claimed with `FOR UPDATE SKIP LOCKED` (so any number of workers
share the queue) and runs its handler in a fresh `Tx`, which also deletes the
row, so a handler that only writes to the database takes effect exactly once. A
failing handler rolls back, backs off (10s, doubling, at most an hour) and is
retried; after `MAX_ATTEMPTS` (5) it is **dead-lettered**: `dead_at` is set and
it stays in `messaging.job` with its `last_error` until someone reruns it. A
claim is a lease, so a delivery whose worker died runs again. Everything is
at-least-once, and **in order per key, per subscription**.

The model is Kafka's: a topic per root, the root's id as the message key,
order per key by the root's `Version` (finer than Kafka's per partition, so a
failing event holds up only its own key), a subscription as a consumer group,
and a `messaging.job` row per message still to deliver in place of an offset
(the relay creates jobs, so nobody needs the outbox in commit order, which
`seq` isn't). It works because **commit order is version order per key**:
every write to an aggregate is a version-guarded `UPDATE` or `DELETE` of its
root's row, so v4's transaction waits on the row lock until v3's commits, and
the outbox row is written in the same transaction; a statement that sees v4's
outbox row sees v3's. Two mechanisms keep that order to the subscriber:

- **One relay at a time.** The relay takes `pg_try_advisory_xact_lock` (another
  worker holding it relays nothing that poll), reads a batch `ORDER BY seq`
  without `SKIP LOCKED`, and in one transaction marks it dispatched and fans
  it out (`INSERT INTO messaging.job … SELECT … JOIN messaging.subscription`),
  each job carrying its event's `key` and `version`. So a key's v3 job exists,
  ran, or was filtered out before its v4 job does. Workers still claim jobs in
  parallel.
- **A claim waits for earlier versions of the same key in the same queue**:
  `NOT EXISTS (SELECT 1 FROM messaging.job e WHERE e.queue = j.queue AND e.key = j.key AND e.version < j.version AND e.dead_at IS NULL)`
  (index `(queue, key, version) WHERE dead_at IS NULL`). A job's row lasts
  until the transaction handling it commits its deletion, so while v3 runs,
  backs off or waits for a worker that can decode it, v4 can't be claimed.

What that gives, and doesn't:
- **No order across keys, topics or subscriptions.** `UserDeleted` and a racing
  `NoteCreated` by that user stay unordered.
- **Gaps are normal**: a filtered subscription, or one registered after some of
  a key's events, sees only some versions.
- **A failing job holds up only its own key, in its own queue**, until it
  succeeds or is dead-lettered (about 2.5 minutes over 5 attempts); then the
  key moves on past it. For a copy of state, skipping it is harmless: the next
  event carries the whole state.
- **Rerunning a dead letter after later versions ran delivers it out of
  order**, and a lease that expires mid-run can run a job beside its successor.

So **a subscriber that keeps a copy of another module's state stores the
version with it** and applies each event in one conditional statement,
`INSERT … ON CONFLICT (id) DO UPDATE … WHERE copy.version < EXCLUDED.version`,
keeping a tombstone row for a deletion. This is a backstop for those two
cases; ordered delivery is the main mechanism. Reactions to facts (mail,
`notes`' archiving) don't compare versions.

A worker claims only what it can decode: each job records its `event_type`,
and the claim takes only the `(queue, event_type)` pairs its own `Handlers`
list (`Handlers::subscriptions`). During a rolling deploy, old and new builds
run side by side; a job for a subscription only the new build has waits for a
new worker, rather than an old one failing it and spending one of its attempts.
A job no running worker can decode simply waits; `NoHandler` stays only as a
guard. The claim looks up each served pair's first due job by index
(`job_due_idx` on `(queue, event_type, run_at, id)`), so jobs nobody running
serves don't slow it. (The release that brought this filter wasn't protected
by it: the build it replaced claimed any queue.)

**Subscriptions live in the database**, in `messaging.subscription`
(`queue`, `event_type`, `topic`, `registered_at`, `retired_at`), and the relay
fans out by joining it, so every build relays the same way whatever handlers
it was built with. They are registered when migrating, never when a worker
starts: the `migrate` bin, after the migrations, calls
`app_db::register_subscriptions(&pool, &handlers)` with every module's
`Handlers` (test drivers call it as they build their worker). Migrations run
before a rollout (§6), so a new build's subscriptions exist before any of its
processes publishes or relays, and there's no window in which events go out
without their new subscribers. A worker never writes the table; on starting
(`Worker::run`, via `check_subscriptions`) it refuses to run if one of its own
`(queue, event_type)` pairs is missing or registered under another topic,
warns of each live subscription it doesn't serve, and still runs the pending
jobs of a retired one it serves.

- **A subscription receives every event relayed after its registration
  commits**, and no history: one added to a running system starts empty.
- **A queue is bound to one topic forever.** Registering an existing queue
  under another topic fails the migrate step; moving to a new topic means a new
  queue name.
- **A retired queue's name is never reused**: registering one fails the
  migrate step, so a rollback or an old `migrate` can't revive it.

**Retiring a subscription** is a migration, never a side effect of deploying
code without its handler: it stops relaying to it, and dead-letters what it
hadn't run yet, where it stays visible:

```sql
UPDATE messaging.subscription SET retired_at = now() WHERE queue = 'notes.archive_deleted_authors_notes';
UPDATE messaging.job SET dead_at = now(), last_error = 'subscription retired'
WHERE queue = 'notes.archive_deleted_authors_notes' AND dead_at IS NULL;
```

Rules:
- A module never writes another module's data, through its `-api` or otherwise;
  it publishes an event, and the owner reacts. Cross-module work is therefore
  eventually consistent: the author check in `notes`' `create` and a deletion in
  `users` are separate transactions.
- No side effect directly after `commit`: publish an event in the `Tx`, and
  do it in a subscriber.
- The worker's time, like the services', is the injected `Clock` (below), so
  tests control backoff.

The **clock** is `app_domain::Clock`, held by services as `Arc<dyn Clock>`, from
the one `Context` the server builds (`SystemClock`) and passes to every
`module(..)`; drivers build it with a `FixedClock` they can `advance()`. Not an
ambient global or task-local: work spawned off a request would silently read the
real time in tests.

### List endpoints

A list use case takes an `app_persistence::ListQuery<XSortKey>` (optional
`PageRequest`, `Vec<Sort<XSortKey>>`) and returns `Listed<T>` (`total` only
when paged). Limits default to 20 and are clamped to 100.

- `XSortKey` is a closed enum in `application/repository.rs`, so no
  caller-supplied text reaches SQL. The db layer maps each key to a fixed column
  and builds the SQL with `QueryBuilder` + `app_db::push_list_clauses` — the one
  sanctioned use of runtime SQL, since `query!` can't vary `ORDER BY` — always
  ending the order in a unique tiebreak.
- Over HTTP the handler takes `limit`, `offset` and `sort: Query<Option<Vec<XSort>>>`
  as separate params (poem-openapi has no parameter structs), builds the query
  with `app_web::paged(..).sorted(app_web::sorts(..))`, and returns
  `ListResult<XResponse>`. `XSort` is a poem `Enum` in `web/models.rs` whose
  values are the response's camelCase field names, `-`-prefixed for descending,
  implementing `SortParam` — so the generated TypeScript union names exactly the
  sortable columns.
- Omit `sort` for an unsortable list, and `limit`/`offset` (with
  `ListQuery::all()`) for an unpaged one.

### Isolation enforcement (layered defenses)

1. **Cargo workspace graph (primary).** A module crate that does not list a
   sibling in `[dependencies]` physically cannot import it. This carries ~80% of
   the isolation for free and is stronger than a test — it won't compile. It
   holds only while nobody adds the line, so
   `scripts/check-module-deps.sh` (`make check-deps`, in `make check` and CI)
   reads `cargo metadata` and fails if a module core's normal or build
   dependencies include another module core (dev-dependencies may: a read
   module's tests run its providers), or if an `-api` crate depends on a
   workspace crate other than a pure `common` crate or another `-api`.
2. **`cargo-deny` `[bans]`.** `sqlx` and `poem` are denied workspace-wide with a
   `wrappers` allow-list of the crates that may depend on them directly
   (`app-db` for sqlx, `app-web` and `server` for poem, and every module core
   crate — read modules included — for both; a new module is added to both
   lists, an `-api` crate never). Every pure crate — the pure
   `common` crates and every `<module>-api` crate — is therefore kept
   framework-free. Bans target third-party crates; the Cargo graph handles
   internal edges.
3. **Per-crate `clippy.toml` (`disallowed-types`/`disallowed-methods`).** In each
   module, bans `sqlx::PgPool`, `sqlx::Pool` and `sqlx::Postgres` from being
   *named* anywhere in the crate; `db/mod.rs`, `di.rs` and `seed.rs` opt out, so
   the application and domain layers cannot. Run with `-D warnings`. This is a
   tripwire, not a boundary: other sqlx types, `poem`, and a local `#[allow]`
   all get past it.
4. **Cross-module use goes through `<module>-api` (D7).** A module that needs
   another depends on that module's pure `-api` crate, never its core crate, so
   the graph still forbids reaching into internals.
5. **`rust_arkitect`** — noted for within-crate layering assertions, but not
   wired: crate-per-module makes it largely unnecessary.
6. **Cross-module SQL goes through published views (D8).**
   `scripts/check-sql-schemas.sh` (`make check-sql`, in `make check` and CI)
   scans each module's `src/db/` and `migrations/` and refuses any
   `<schema>.<object>` that is neither the module's own schema, its own
   `<schema>_api`, nor a view listed in its `[package.metadata.module] reads`.
   Only a read module may declare `reads`: `make check-deps` fails a module
   with a `src/domain/` (a write module) that does, so a write module never
   joins across schemas.
7. **Frontend feature boundaries (§8).** `frontend/scripts/check-feature-imports.mjs`
   (`pnpm run lint:imports`, part of `make lint-frontend`, `pnpm run lint` and
   CI) resolves every `@/` and relative import under `src/` and fails if a
   feature imports another feature, or if code outside a feature imports a
   file inside one rather than its `index.ts`. A script, because Biome 1.9's
   `noRestrictedImports` matches exact module names only.

---

## 5. Common crates

The split axis is **pure vs framework-coupled** — a pure crate can be used by the
domain without dragging in poem/sqlx, which is what lets the bans in §4 hold.

| Crate | Coupling | Contents | Who depends |
|---|---|---|---|
| `app-domain` | pure | domain building blocks: `uuid_newtype!` for typed ids (add `new()` only to ids the module mints), the `Clock` trait (`SystemClock`, and `FixedClock` for tests; at `timestamptz` precision; read only by services and the worker), `well_known_id` and `seed_time` for seeds | domain and application layers |
| `app-error` | pure | `thiserror` canonical error kinds (`AppError`, `AppResult`) | anyone (incl. domain, application) |
| `app-events` | pure | `Topic` and `topic!` (a root's events, keyed by its id), `Event` (one of them), `EventPayload` / `NoContext` (what it carries: the root's state and a context), `EventEnvelope` / `EnvelopeOf<E>` / `TopicEnvelope<T>` (the CloudEvents 1.0 envelope it is stored and delivered in) (§4) | `-api` crates, application layers, `app-persistence`, `app-db` |
| `app-persistence` | pure | `Version` / `Versioned` / `Expected` (optimistic concurrency), `ListQuery` / `PageRequest` / `Sort` / `Listed`, the unit of work (`Tx`, `Transactions`), `EventPublisher` / `PendingEvent`, `Handle` / `Handlers` (subscribers, §4) | application and db layers |
| `app-web` | poem | `app-error` → HTTP mapping (`ApiError` / `ApiResult`, the shared `ErrorBody`, `render_error` for framework errors), `WithETag` / `if_match_version`, list wire types (`ListResult`, `SortParam`, `paged` / `sorts`), `Provide` / `Accept`, request-id middleware, `/health`, **JWT `SecurityScheme`**; behind the `test` feature, `testing` (the transport half of test drivers, §13) | web layers, `di.rs`, server; test drivers |
| `app-db` | sqlx | `connect`, `ping`, the per-schema migration runner (`run_migrations`, `ModuleMigrations`; always applies its own `messaging` schema first), `push_list_clauses`, `map_db_error` (constraint name → `AppError`), `PgTransactions` / `conn` (the unit of work over `sqlx::Transaction`), `PgEventPublisher` (the outbox), `Worker` (relay, delivery, dead letters) | db layers, server |
| `app-module` | poem + `app-db` | `Context` (the pool and the clock every module is built from), `Module`: a module's provider, handlers, migrations and seed, registered once with the server (§4) | `di.rs`, server, test drivers |
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
  (`users`, `notes`, …). A module that publishes views for other modules' SQL
  creates them in a second schema, `<schema>_api` (D8).
- **Per-module migrations**: each module crate owns a `migrations/` directory.
  `server/src/bin/migrate.rs` applies them in a deterministic, dependency-aware
  order via the `app-db` runner. `sqlx::migrate!` embeds them at compile time,
  so each crate with migrations has a `build.rs` that reruns on `migrations/`
  changes — without it Cargo can reuse a build that lacks a new migration.
- **Offline compile**: the `.sqlx/` query cache is checked in and
  `SQLX_OFFLINE=true` is the default, so `cargo build` and CI need **no live
  database**. `make prepare` regenerates the cache against the docker Postgres.
- **Migrations run explicitly** via `make migrate` — never silently on server
  startup, so production controls migration timing.
- **Seeding non-prod environments** follows the same rule: `server`'s `seed` bin
  (`make seed`, which depends on `migrate` and which `make dev` depends on) runs every module's
  `seed::standard`. It exists only with `server`'s `seed` feature and refuses
  `APP_ENV=prod`. The image ships `migrate` (and, with `--build-arg SEED=true`,
  `seed`) beside `server`; a deployment runs them as one-off pre-start tasks
  inside its network (init container, pre-deploy job), never from CI.
- **The worker** (§4) ships in the same image: `/usr/local/bin/worker`, run as
  its own long-lived deployment beside `server`, with the same config
  (`DATABASE_URL`, `APP_*`; `APP_WORKER_POLL_MS` sets its idle poll, default
  1000). Scale it by running more replicas: they share the queue. It needs the
  migrations applied, like the server, and its subscriptions registered (which
  `migrate` does; it refuses to start otherwise), and stops on SIGTERM after the job in hand.
  `make dev` runs it beside the server.
- **The `messaging` schema** (outbox and delivery queue) belongs to no module: `app-db`
  owns its migrations, and `run_migrations` applies them before any module's.

---

## 7. OpenAPI spec generation

- The **code is the source of truth**. `server` builds an `OpenApiService` from
  the `Endpoints` tuple in `modules.rs` (§4), built with `Endpoints::default()`
  — group into sub-tuples past 12 modules, where std stops deriving `Default`
  (poem-openapi's own cap is 16).
- `OpenApiService::spec()` returns the spec JSON **synchronously, with no server
  and no async runtime**. `dump-spec.rs` constructs the *same* service
  configuration (shared `build_service()` fn — identical `title`/`version`/
  `server` url as runtime) and prints the JSON to stdout.
- Because handlers take their service via `Data<&XService>` and `Endpoints`
  structs are unit structs, **`dump-spec` needs no database and no pool** — spec generation is a
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
- **Errors are part of the contract.** Every operation returns
  `app_web::ApiResult<T>`, so the spec declares the same error responses
  (400/401/403/404/409/412/428/500) on each, all with one `ErrorBody` schema:
  `code` (a closed `ErrorCode` enum, the thing to branch on), `message`, optional
  `fields: [{path, message}]` naming the inputs at fault, and `requestId` (also
  the `x-request-id` header). poem/poem-openapi's own failures (bad JSON, a
  missing field, an unknown enum value, failed auth, unknown routes) go through
  `app_web::render_error` via `catch_all_error`, so they have the same shape
  (`malformed_request` for unparseable input). The generated client therefore
  types every error as `ErrorBody`. A flat body rather than RFC 9457
  `problem+json`, and one error set for every operation rather than only the
  statuses each can return: simpler, at the cost of precision.
- **Forms**: TanStack Form. A form saves in `validators.onSubmitAsync` and returns
  `submitErrors(error)` (`src/lib/api-errors.ts`), which places each field error
  on the input its `path` names; the mutation's `meta.formFields` lists those
  inputs, and the `QueryClient` from `src/lib/query-client.ts` toasts every other
  failed mutation. `NewNoteForm` is the reference.
- **Wire types**: the domain uses `Uuid` and `chrono`, but request/response
  types expose ids, timestamps and versions as plain `string`, so the generated
  client needs no date or uuid dependency.

**Code organisation.**
- **By feature, mirroring backend modules.** Module `modules/<module>` gets
  `src/features/<module>/`: its components, hooks and tests, with an `index.ts`
  re-exporting the public components (import `@/features/notes`, never a file
  inside it). Features never import each other (`pnpm run lint:imports`
  enforces both, §4); shared code lives in
  `src/components/`, `src/hooks/` or `src/lib/`. A feature that needs another
  module's data calls that module's generated client directly — the API is the
  contract, as `<module>-api` is on the backend.
- **Pages** (`src/pages/<name>-page.tsx`) are thin: they lay out features and
  own URL concerns (params, search). Promote one to a folder only when it grows
  private pieces.
- **Routing** is TanStack Router, code-based (no file-route codegen): routes
  live in `src/router.tsx`, and `router.test.tsx` renders
  `createAppRouter(createMemoryHistory(..))` to check each URL lands on its
  page. The prod server falls back to `index.html`, so deep links work (§9).

**Design system: shadcn/ui** (Base UI primitives, Tailwind v4, `base-nova`
style; config in `components.json`). The whole registry is vendored as source in
`src/components/ui/` (plus `src/hooks/`, `src/lib/highlighter.tsx`): use an
existing component before writing one, and compose rather than restyle. Theme by
editing the CSS variables in `src/index.css` (`:root` / `.dark`), not
per-component classes. `Toaster` and `TooltipProvider` are mounted in
`main.tsx`. Refresh from upstream with `pnpm dlx shadcn@latest add --all --overwrite`,
then `pnpm run format` (the CLI writes its own style) and re-check local edits.
`biome.json` disables the specific rules upstream code trips for the vendored
paths only; app code keeps the full rule set.

**Lists** use `src/components/data-table/` (TanStack Table, server-paged and
server-sorted against §4's list endpoints): `useListParams<XSort>({ sortable: [...] })`
holds the table state and yields the `query` for the generated `getXOptions`
(`sortable` is typed against the generated sort union), `useDataTable` binds the
`ListResult<T>` response, and `DataTable` / `DataTablePagination` /
`DataTableViewOptions` render it. `src/features/notes/notes-table.tsx` is the
reference.

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
- `make dev` — depends on `seed` (which depends on `migrate`) + `generate`; then
  `make -j` runs `cargo watch` (backend `:8080`), `cargo watch` on the `worker`
  bin, **and** `vite` (`:5173`) in parallel.
- `make generate` — alias for the spec-dump + codegen chain. The FE targets
  (`typecheck`, `test-frontend`, `build-frontend`) depend on it, so they never
  run against a stale client.
- `make watch-api` — optional: watches backend, auto-regenerates the client.
- `make migrate` / `make prepare` — DB migrations / sqlx offline cache refresh.
- `make new-module NAME=..` — scaffold and register a write module (§4);
  `make check-modules` checks every module is registered.
- `make check` — the CI mirror, every step of both CI jobs (§14).
- `make build` — `vite build` then `cargo build --release` (embeds the SPA).

**Repo-local tooling.** Rust CLI tools (`cargo-deny`, `cargo-watch`,
`sqlx-cli`) are pinned in the root `Cargo.toml` `[workspace.metadata.bin]` and
installed by `cargo-run-bin` into a gitignored `./.bin`, so every clone and CI
run uses the same versions and nothing goes into `~/.cargo/bin` but the one
bootstrap. `make setup` installs it and the tools; the Makefile runs them as
`cargo bin <tool>`. Frontend tools are repo-local already via `package.json`.
`lefthook` (a Go binary) is the exception and is installed separately.

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

- `server::telemetry` initializes `tracing-subscriber`: JSON output in prod, pretty
  in dev.
- `poem`'s `Tracing` middleware inside a request-id / correlation-id
  middleware in `app-web` (outermost, so access logs and every response, errors
  included, carry the id; error bodies echo it as `requestId`).
- `/health` (and readiness) endpoint in `app-web`.
- OpenTelemetry export left as a commented seam in `server::telemetry`.

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
  ├── main.rs        # mod driver; mod behaviours { mod registering; … }
  ├── driver.rs      # the only code that knows routes, JSON, statuses, tokens
  └── behaviours/
      ├── registering.rs # one file per behaviour area…
      └── …
  ```

  The target is named after the module because `#[sqlx::test]` derives each
  test's database name from its path (`users::behaviours::registering::…`): a
  name shared across modules would give same-named tests the same database.

  The **driver** boots the module's `Endpoints` over the per-test pool
  (`Notes::seeded(pool)`: migrate, seed, build), exposes domain verbs
  (`notes.edit(ALICES_NOTE, version, ..)`, `users.signed_in_as(ALICE).me()`), and
  turns each response into `Result<XJson, Refusal>`, declaring only the variants
  the module produces from `Invalid` (with the paths of the fields at fault),
  `Malformed`, `NotFound`, `Conflict`, `NotSignedIn`, `PreconditionRequired` and
  `Stale`. It maps the error body's `code`, not the status (`From<Refused>`), so a
  broken domain rule (`Invalid`) is told apart from an unparseable request
  (`Malformed`). The transport is shared: `app_web::testing`, behind `app-web`'s
  `test` feature (dev-dependencies only), mounts the `Endpoints` with the server's
  error rendering and request ids (`TestApi`), builds requests (`.json`,
  `.if_match`, `.bearer`, `.queries`), and on `send()` (`send_empty()` for a
  bodyless success such as a delete) checks that every refusal
  has the error shape, echoes the request id and names fields only for a
  validation error. A driver keeps only its verbs, its `Refusal`, its stubs and
  its response newtypes.
  A response newtype (`json_newtype!(pub NoteJson)`; `Page<NoteJson>` for a list)
  wraps the returned JSON (`Deref<Target = serde_json::Value>`), so a test
  can compare it whole against a seed aggregate, or — when some properties can't
  be predicted, like a generated id or timestamp — assert on parts of it with
  `app-test-support`'s JSON path macros (strict match against the first value a
  path selects).
- **Time and background work.** A driver builds its module's `Context` with a
  `FixedClock` (after the seeds' times) and the module's worker
  (`ctx.worker(module::handlers(&ctx, ..))`, its subscriptions registered first
  with `app_db::register_subscriptions`, as `migrate` does), so tests stay synchronous and
  deterministic: work happens only when a verb runs it (`run_pending_events()`,
  i.e. `Worker::run_until_idle`) and time moves only when one
  advances it (`later(by)`). A subscriber's driver stands in for the publisher
  by publishing the `-api` event itself (`notes.user_deleted(ALICE)`), as it
  stubs a `-api` trait; the publisher's driver exposes what it published as
  `announced::<E>()` (over `Worker::published::<E>()`), each paired with the
  version it announces (`[(Version::from_i64(2), announced(&note))]`; not the
  whole envelope, whose `id` is random), every state-changing
  behaviour asserts its event and version and every refused one that nothing was announced,
  and dead letters are `Worker::dead_letters()`.
  A port to an external system gets a Fake in the driver (`users`' `FakeMailer`,
  which can be taken down to exercise retries). The journeys (below)
  run reactions for real across modules.
- **Tests speak the domain, not HTTP.** Names state a behaviour
  (`an_edit_without_a_title_is_rejected_and_changes_nothing`); bodies call driver
  verbs and compare against seed aggregates (`Ok(seed::alices_note().into())`).
  One behaviour per test; a refused action is followed by a check that nothing
  changed.
- **Journeys: the modules compose.** Each module's tests stand in for whatever
  is across its boundary — a subscriber's driver publishes a hand-built provider
  event, a consumer stubs the provider's `-api` trait, a read module joins views
  over seeded rows — so none of them fails when a publisher's payload drifts from
  what its subscriber expects, a provider's real `-api` implementation differs
  from the stub, a write doesn't reach the views a read module joins, or the
  server's own wiring (`modules.rs`, the providers passed in, the composed
  worker's subscriptions, real JWT auth) is wrong. A **journey** is one test that
  walks a path across modules on the composed app, asserting at each step. It is
  the one carve-out from "one behaviour per test": a behaviour test checks one
  rule within a module; a journey checks that modules compose along a path, each
  step's result feeding the next and every step asserted. Journeys don't re-test
  rules, so they are few, only for paths that cross a boundary, and named for the
  path walked:
  - `a_note_is_found_by_its_authors_current_name`: Alice writes a note, search
    finds it under her name, she renames herself, and search finds it under the
    new name and not the old (notes' and users' write paths, both published views,
    note-search's join);
  - `deleting_an_account_archives_only_that_users_notes`: Alice deletes her
    account, her note is unchanged until the worker runs, then it is archived,
    Bob's isn't, and nothing is dead-lettered (users' real `UserDeleted` payload
    into notes' real subscription).

  They live only in one target, `journeys` (`crates/server/tests/journeys.rs`,
  its driver beside it in `tests/driver.rs`, and one file per journey area in
  `tests/journeys/`). The root declares them in an inline `mod journeys { … }`,
  so each test path starts `journeys::` and `cargo test -p server journeys::`
  runs only them; the target's name keeps its database names apart from the
  modules'. `server`'s `Cargo.toml` sets `autotests = false` and lists its test
  targets, or `tests/driver.rs` would build as a target of its own. The driver follows the
  module drivers' discipline: `App::seeded(pool)` migrates, seeds and registers
  the subscriptions of `server::modules()`, then builds `server::build_app` and
  `server::worker` over a `Context` with a `FixedClock`, mounted over
  `app_web::testing` (`TestApi::over`), so every refusal is checked for the error
  shape and request id. Its verbs cross modules and act as a signed-in user with
  a real token (`app.as_user(ALICE).write_note(..)`, `.rename_myself(..)`,
  `.delete_account()`, `app.search_notes(q)`, `app.note(id)`,
  `app.run_pending_events()`), carrying each version from response to `If-Match`
  themselves, and return `XJson`/`Refusal` shapes as the module drivers do. No
  route, method, status or header appears outside `driver.rs`. A path within one
  module is a behaviour test in that module (`notes`' `behaviours/lifecycle.rs`
  writes, revises and removes a note), and neither a module nor the template has
  journeys: a journey depends on which modules the app has. A module that reacts
  to, provides for, or is read by another adds or extends a journey.
  `composed_app.rs` stays beside them as a wiring probe over the spec, not a
  journey.
- No separate service tests: every use case is reachable through the `Endpoints`
  or the worker verbs, and
  a fresh database costs the same either way. Add one only when a use case has
  neither, or two outcomes are indistinguishable over
  HTTP.
- No in-memory Fakes for Postgres. A hand-written **Fake** behind an interface is
  reserved for an external dependency that cannot be run locally. The other
  sanctioned stand-in is a consumer's driver stubbing a `<module>-api` trait (D7).

**Backend — seeding (first-class).**
- Every module that owns data owns `src/seed.rs`, compiled only with its `seed`
  cargo feature. The crate enables the feature for its own tests with a self
  dev-dependency (`users = { path = ".", features = ["seed"] }`); production
  builds never do. A module that owns no data (`note-search`) has no seed.
- A seed module exposes: a **named `const` id** per well-known entity
  (`users::seed::ALICE`), a function returning that aggregate
  (`seed::alice() -> User`) for expected values, and `seed::standard(&pool)`,
  which saves whichever of the standard dataset is missing through the real
  repository — so the `seed` bin can re-run it on every deploy (§6).
- Seeds are built with the real domain constructors — which take the id and
  timestamp as arguments — so they satisfy every invariant and there is no second
  copy of the SQL. Timestamps are fixed and spaced apart so ordering is
  deterministic.
- Drivers start from `run_migrations(..)` + `seed::standard(..)`; tests address seeded
  data by constant, never by ids captured at runtime.
- Well-known ids are readable UUIDs: `MMMMMMMM-KKKK-0000-0000-NNNNNNNNNNNN` —
  module number, entity kind within the module, sequence (e.g. Alice is
  `00000001-0001-0000-0000-000000000001`), built with
  `app_domain::well_known_id(MODULE, kind, sequence)`. Each module with a seed
  declares its number once, as `pub const MODULE: u32` (in its `-api` seed if it
  has one, else its core seed), and its kinds in that seed's doc comment;
  `make check-modules` fails on a missing number or one two modules share, and
  `make new-module` allocates the next one.
- **What other modules may know of a seed is published in its `-api`** (D7).
  Under the `-api` crate's own `seed` feature (enabled by the core's),
  `users_api::seed` holds the well-known ids as plain `Uuid`s and, per entity, a
  function returning its **published record** (`users_api::seed::alice() ->
  v1::User`). The core seed builds each aggregate from that record, adding what
  isn't published (a user's registration time, a note's body), so the data has one source. Saving
  stays in the core: `-api` stays pure.
- A seed may name another module's seed ids **only through that module's
  `-api`**, along the dependency graph: `notes`' seed authors notes as
  `users_api::seed::{ALICE, BOB}`, and `notes`' driver stubs `UserDirectory` with
  exactly those users. A seed saves only its own module's data. Tests of a
  module that reads others' data (a read module) run the providers' migrations
  and `seed::standard` and assert against their `-api` seed records, never their
  aggregates.

**Backend — inline unit tests (the ~10%).**
- Reserved for pure, side-effect-free logic — aggregates and value objects in the
  domain layer are the natural home. Where the state space is combinatorial, use
  **`proptest`** (property-based) rather than example tables.
- The one exception is `app-persistence`'s subscriber registry (`Handlers`):
  routing a delivery to its queue's typed handler is shared plumbing every
  module relies on, so it is tested inline against a hand-written `FakeTx`
  `TxBackend` rather than only through each module's worker verbs.

**Frontend — Vitest, minimal mocking.**
- Prefer rendering real components/hooks. When a test must cross the network,
  mock at the **network boundary with MSW** — never module/import mocks — so the
  real generated client stays in the path. Node's fetch rejects the client's
  relative `/api` base, so `vitest.setup.ts` gives it an absolute one.
- Feature tests render the feature's components against MSW; page-level
  behaviour is only which URL renders what (`router.test.tsx`).

---

## 14. Production build & CI

**Dockerfile (multi-stage):**
1. `node` stage — `pnpm install` + `vite build` → `frontend/dist`.
2. `rust` stage — `cargo build --release` of `server` (embeds `dist` via
   rust-embed), `worker` and `migrate` (and `seed` with `SEED=true`),
   `SQLX_OFFLINE=true`.
3. Runtime — `gcr.io/distroless/cc`, copies the binaries; `server` is the
   entrypoint. Tiny, secure, glibc (avoids musl/`aws-lc-rs` toolchain friction).

**GitHub Actions `ci.yml`** runs the same steps as `make check`:
- `cargo fmt --check`
- `cargo clippy --all-targets --features server/seed -- -D warnings` (incl.
  per-crate `disallowed-types`; the feature brings the `seed` bin in)
- `cargo deny check`
- `scripts/check-sql-schemas.sh` (D8)
- `scripts/check-module-deps.sh` (§4)
- `scripts/check-modules.sh` (§4)
- `cargo build --features server/seed` with `SQLX_OFFLINE=true`
- `docker compose up -d --wait postgres`, then backend tests with
  `DATABASE_URL` → the compose server's `postgres` database
- frontend: `pnpm install`, `make generate`, `tsc -b --noEmit`, `biome ci`,
  `pnpm run lint:imports`, `vitest run`, `tsc -b --noEmit && vite build`
- scaffold: `make new-module NAME=gadgets`, then its clippy and tests and the
  composed-app test (CI only: it edits the tree; the template itself is covered by
  the backend steps)

A step added to one must be added to the other.

**Git hooks** — `lefthook` pre-commit: rustfmt, clippy (same features as CI),
biome on staged files, `tsc`.

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

No foreign keys between modules: a module's tables never reference another
module's tables or views. A cross-module id is a plain `uuid` column, checked
when written through the provider's `-api` trait where the business rules need
it (D7). References can dangle — a user is removed — and every consumer
tolerates that (a note-search result without an author name). A foreign key
would couple migration order, deletes and locks across modules and stop one
being lifted out; `make check-sql` refuses one, as it does any cross-schema
reference that isn't a listed view (§4.6).

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

### D5 — Handlers take their service via `Data<&XService>`
**Decision.** `Endpoints` structs are unit structs; each module's provider
(`di.rs`) builds its service over the pool, and the server injects it as request
data. **Why.** Makes `dump-spec` a pure, DB-free, sub-second step — the fast
inner loop depends on it. Handlers never see the pool, so the web layer needs no
sqlx; testability comes from the `Repository` trait, not from state on the
`Endpoints` struct.

### D6 — Application layer between web and domain
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
lost-update window under concurrent writes, so every write module closes it with
a version column and `If-Match` (§4, Optimistic concurrency); the version stays
out of the domain, in `app-persistence`'s `Versioned` wrapper.

### D7 — Cross-module dependencies go through a pure `<module>-api` crate
**Decision.** A module that other modules may use is split into two crates under
`crates/modules/<module>/`: `api/` (package `<module>-api`) and `core/`
(package `<module>`). The `-api` crate holds only what consumers may see —
capability traits (`users_api::UserDirectory`), published records and seed
descriptions (below) — and is pure (no poem/sqlx, enforced by cargo-deny). Ids
cross it as plain `Uuid`: the provider's typed id (`UserId`) stays in its domain,
and each consumer keeps its own typed id, so the newtype protection lives on both
sides and the seam is a single `as_uuid()`. Consumers depend on `-api`, never
on the core crate. The core crate implements the traits (in its application
layer) and exposes a constructor from its composition root
(`users::directory(pool)`); the server passes it to each consumer's
`module(..)` (`notes::module(users::directory)`), which hands it to the provider. A consumer keeps its own
domain types (`notes::AuthorId`) and converts at the call in its application
layer, so its domain names no other module. Consumers' tests stub
the trait in their driver (the other module isn't run); the provider tests its
implementation against Postgres directly, since it has no HTTP entry point.

**Which mechanism.** The choice follows *why* the data is needed, not how much
of it. When a module's business logic needs to know something about another
module's data, to decide or validate, it calls the provider's trait — at any
size: a bulk need gets a batch method (`users_exist(ids)`, implemented with
`WHERE id = ANY($1)`), not a view. When an endpoint presents data spanning
modules — listing, searching, filtering, sorting or paging by another module's
columns — it lives in a read module over published views (D8). A write module
never reads views, and never joins across schemas; a list it can't serve from its
own schema moves to a read module.

**Published records.** Each published view (D8) has a plain Rust row type in
the provider's `-api`, in a module named for the view's version:
`users_api::v1::User` is a row of `users_api.user_v1`. It is written with its
version path (`v1::User`), never imported bare, so it can't be mistaken for the
aggregate, and is `#[non_exhaustive]`: consumers never build one, so the field a
view gains can't break them. A breaking change is `v2::User` beside `v1::User`,
as `user_v2` beside `user_v1`. Records describe the data product only: a trait
returns its own types, so a view gaining a column never changes a trait's
signature or a consumer's stub. Under a `seed` feature the `-api` also publishes
the module's well-known seed ids and records (§13); consumers' seeds and tests
name another module's seed data only through it.

**Why.** The Cargo graph keeps enforcing that no module
reaches into another's internals, while giving cross-module calls a small,
named, versionable surface. Contracts between crates also allow two modules to
use each other without a crate cycle. Publishing seed data in `-api` lets data
line up across modules (notes by Alice) along the same edges the code already
has, without a consumer depending on a provider's core or constructors.
**Alternatives rejected.** Depending on
the sibling's core crate (drags in its poem/sqlx and all its internals);
`api/`/`private/` folders inside one crate (nothing enforces them); a
consumer-owned port with every adapter in `server` (fine for one edge, but the
glue piles up in `server`); choosing by size, views for anything bulk (puts
views, and the providers' migrations, into write modules' validation and
tests). **Naming.** `-api` names the in-process capability
surface; the HTTP layer inside a module is therefore `web/`, with
`XEndpoints` structs.

### D8 — Cross-module reads go through published views and a read module
**Decision.** When a read must combine data owned by several modules — filter,
sort or page notes by their author's name — the join happens in Postgres, not in
application code. Each provider **publishes** what others may read as versioned
views in a schema of its own, `<schema>_api` (`users_api.user_v1`,
`notes_api.note_v1`), created by its own migrations: only the columns it is
willing to support, never its tables. A published view is a **data product**: a
versioned, read-only dataset the module publishes for querying (read modules
now, perhaps reporting or exports later), as opposed to a trait, which is a
capability shaped by one consumer's use case (D7). A view evolves only additively
(`CREATE OR REPLACE VIEW` may append columns); a breaking change ships as `_v2`
beside `_v1` until every reader has moved, and its published record (D7) moves
with it. A **read module** (`note-search`)
does the joining: it owns no source data and no domain layer — just
`web/`, `application/` (a service and a `NoteFinder` trait returning read
models, not aggregates) and `db/` (the SQL). It reads other modules' data only
through views, even for a single row, never calls `-api` traits, and never
writes another module's data. It declares every view it reads in
`[package.metadata.module] reads` in its `Cargo.toml`, and `make check-sql`
refuses any other cross-module reference (§4.6). It has no Rust dependency on
the providers in production; its tests run the providers' real migrations and
seeds (dev-dependencies on their core crates — the only core-to-core edge, and
test-only) instead of stubbing, because the views are what is under test, and
assert against the providers' `-api` seed records (§13). A read module that owns
no data has no migrations and no seed (`note-search`); one may later own
*derived* data — embeddings, search indexes, projections built from the views it
reads — and then has its own schema, migrations and seed like any module. The
server registers it like any module (§4). **Why.** Joins, sorting and paging across
modules stay one indexed query, always transactionally consistent — no
projection to rebuild, no events, no lag — while each provider keeps its tables
private and its published surface small, named and versioned, exactly as
`<module>-api` does in Rust (D7). Keeping the join in a read module rather than
in `notes` keeps the write modules' tests and schemas free of each other.
**Alternatives rejected.** Composing in application code (fetch matching ids
from `users-api`, then `WHERE author_id = ANY(..)`): fine for a filter, but
can't sort or page by the other module's columns. A BFF calling each module:
the same limitation, plus N+1 calls. Event-carried state transfer into a
projection: the right tool once modules are separate services, but a lot of
machinery and eventual consistency for a monolith that shares one database.
Letting `notes` join `users_api` itself: works, but couples the write module's
tests and migrations to users'. Creating the views from the `-api` crate's own
migrations: a view depends on the tables at a given version, and two migrators
per module have independent version sequences, so nothing keeps them ordered
(and `-api` would need sqlx). **Trade-offs.** Providers' migrations must keep
the published views compiling (Postgres refuses to drop or retype a column a
view uses — the intended tripwire). The read module and its providers share one
database, so lifting a provider out (D1) means replacing its views with a
replicated copy first. The SQL check is a grep, not a parser.

### D9 — Units of work, a transactional outbox and a Postgres delivery queue, hand-rolled
**Decision.** Writes run in a pure, opaque unit of work (`app_persistence::Tx`);
the use case publishes its events through its own port,
`app_persistence::EventPublisher`, into that unit of work, and `app-db` writes
them to one shared outbox (`messaging`) in the same transaction, and a `worker` bin fans
each out to its subscribers' queues and delivers it (§4). Events crossing modules are typed, versioned
structs in the publisher's `-api`, stored as JSON. **Why.** Atomicity of "save,
then do something" is the database's to give, and only a shared transaction can
give it; a typed event in `-api` is the same kind of small, versioned, compiled
contract as a trait or a published record (D7), and keeps modules writing only
their own data. **Hand-rolled, not a crate.** `sqlxmq` (0.6) is on `sqlx` 0.8, so a second sqlx
and a second pool; `pgmq` needs a Postgres extension the stock image lacks.
`apalis-postgres` 1.0 is on `sqlx` 0.9 and is the one to revisit, but it is a job
runner, not an outbox: the event fan-out to subscribers, enqueueing inside the
use case's own transaction, and the injected clock tests depend on would still be
ours, around its own schema and worker model. What's left to hand-roll is two
tables and three statements (`relay`, claim with `FOR UPDATE SKIP LOCKED`,
complete or fail), tested here. **One shared
`messaging` schema** rather than an outbox per module: one relay query, one place
for dead letters, and no module writing another's schema when an event fans out
into the subscriber's queue; it belongs to no module, so it is `app-db`'s, and
events are namespaced by topic. Lifting a module out (D1) means relaying its
events to a broker instead, which is what an outbox is for. **Publishing is a port beside the
repository, not a method on the `Tx`**, so the service publishes deliberately
and the `Tx` stays a plain transaction that knows nothing of events or the
outbox; the publisher writes the outbox row immediately, as a repository writes
its row, and a rollback discards both. **Each subscription is its own queue**, so one failing
subscriber neither blocks nor retries another. **No jobs.** A job queue for a
module's own deferred or scheduled work was built and removed: its only use, a
welcome email, was a command dressed as an event (`UserRegistered` replaced it),
and nothing needed scheduling. **Alternatives rejected.** A
`Tx`-taking `-api` trait to let modules share a transaction (couples their
transactions, and can't survive lifting one out); opaque JSON events with a
schema registry (no compiler between publisher and subscriber); passing
`PgPool` into services (defeats the clippy tripwire); a closure-scoped
`transaction(|tx| ..)` (async closures over `dyn` services don't compose; a
dropped `Tx` already rolls back). **Ordered per key, by topic, Kafka's
model.** A topic per root with an exhaustive event enum lets a consumer see
one entity's events together; the root's version orders them, because
version-guarded writes already make commit order version order per key, so
one relay at a time and a claim that waits for a key's earlier versions are
all it takes. No partitions (a failing event holds up only its key) and no
offsets (they'd need the outbox in commit order; per-key order doesn't).
**Subscriptions are rows, registered by `migrate`**, so old and new builds
relay alike during a rolling deploy, and a worker claims only what it can
decode. **Trade-offs.** Delivery is at-least-once, ordered only per key and
subscription (rerun dead letters and expired leases can still reorder, so a
subscriber keeping a copy of state compares versions), so every handler is idempotent; cross-module consistency is eventual
(a note created by a user deleted at that moment can escape archiving); a
handler holds a transaction open while it runs, so a slow external call holds a
connection; the outbox keeps dispatched rows until someone prunes them.

---

## 16. Open items / future variants (not built by default)
- S3/CloudFront static-hosting deploy path (alt to embedded binary).
- OpenTelemetry exporter (seam present).
- Playwright E2E.
- Kafka (or other) module external deps — add them as docker-compose services
  the same way; testcontainers remains an option for deps tests must own.
- Migrate + seed once into a template database and clone it per test, if
  per-test migration becomes the bottleneck.
- Per-module Postgres roles, each granted `SELECT` on other modules' `_api`
  schemas only — the database-enforced version of `make check-sql` (D8).
- Search indexes for read modules (`pg_trgm` for `ILIKE '%…%'`, or a
  `tsvector` column published through the view) when data volume needs them.
- `rust_arkitect` within-crate layering tests, if any module keeps layers as
  sub-modules instead of crates.
- `LISTEN`/`NOTIFY` to wake the worker on commit instead of polling.
- Pruning dispatched outbox rows, and an admin surface for dead letters
  (today: SQL on `messaging.job`).
- Starting a new subscription from history: Kafka's `earliest` (copy a
  topic's retained outbox rows into the new queue on registering) or
  `compacted` (only each key's newest version, the cheap way to bootstrap a
  copy of state; overlaps with published views, D8, so it needs its own
  decision). Today a subscription starts empty (§4).
- Scheduled and recurring work ("do X at T", cron-like), once a module needs
  it: rows in `messaging.job` with a future `run_at` already wait their turn, so
  it's a `Tx` method and a `Handlers` registration on the existing queue.
- A relay to an external broker (Kafka, SNS) for a module lifted out (D1, D9).
- Read modules owning projections fed by events, once a provider is lifted out
  and its published views (D8) can no longer be joined.
