# Rust + React template

A clone-and-go template: a Rust modular-monolith backend whose OpenAPI Spec drives a type-safe React frontend. Terms here have one meaning across the codebase; if code or conversation drifts from them, the drift is the bug. This file is a glossary only: decisions and their reasons live in `docs/platform/adr/`. The business domain's terms, once there is one, live in the root `GLOSSARY.md`.

## Architecture

**Modular Monolith**:
The overall architecture: many isolated Modules compiled and deployed as a single artifact. Isolation is real, enforced by the compiler and CI (ADR 0017), not merely conventional; the single artifact is a deployment choice, not an isolation compromise.

**Module**:
A self-contained unit of the business domain (e.g. `users`, `notes`). Each is its own crate and owns everything it needs: its HTTP surface, its business logic and its persistence, including its database schema and migrations. Designed to be **liftable**: extractable into a standalone service with little surgery, because it declares its own dependencies and never reaches into another Module's internals.

**Layer**:
A horizontal concern within a Module. There are exactly four (a Read Module has no Domain Layer):
- **Web Layer**: the Module's HTTP surface, its operations and request/response shapes. The only Layer that knows the web framework. Translates requests into Commands and hands them to a Service.
- **Application Layer**: the Module's use cases, as Services. Defines the Repository interfaces. Knows nothing of HTTP or the database.
- **Domain Layer**: the business rules, as Aggregates and Value Objects. Knows nothing of HTTP, the database or Repositories. Depends only on pure crates.
- **DB Layer**: persistence. The only Layer that knows SQL and the database driver. Implements the Repository interfaces.
_Avoid_: Tier

**Common**:
Shared cross-cutting capability owned by no Module, split into focused crates. A **pure common** crate carries no web or database dependency and may be used by any Layer, the Domain included. A **framework-coupled common** crate carries one, and only the Layer sharing that concern may use it.
_Avoid_: Shared, utils

**Server**:
The single deployable unit. It assembles every Module's HTTP surface into one service, applies cross-cutting middleware, serves the Spec, and hosts the compiled frontend. It holds no business logic.

## The domain

**Aggregate**:
A domain object that owns its own consistency: its state changes only through its domain actions, each of which either succeeds or returns a domain error and leaves it unchanged. It is what a Repository loads and saves as a whole, never a row or data-transfer object.
_Avoid_: Entity (for the whole), model, record

**Aggregate Root**:
The entity at the top of an Aggregate, through which everything in it is loaded, changed, saved and announced. It draws the consistency boundary: what must change together, in one transaction, under one version. It has a Repository, a Topic and the Aggregate's only version. A Module may own several (ADR 0030).
_Avoid_: Parent (except relative to a Child Entity)

**Child Entity**:
An entity with an identity inside an Aggregate, owned by its Aggregate Root (e.g. an Item of a Note). It has no Repository, Service, Topic or version of its own, carries its root's ETag, and is announced by Events on its root's Topic.
_Avoid_: Sub-aggregate, nested resource

**Value Object**:
A domain type defined by its value rather than an identity, valid by construction (e.g. an email address). Holding one proves it passed validation.

**Command**:
A named intent to change the domain (e.g. register a user, edit a note). Built by the Web Layer from a request and passed through a Service to an Aggregate's domain action.

**Service**:
An Application-Layer type that carries out a Module's use cases. For each it asks a Repository for an Aggregate, invokes a domain action with a Command, and saves the result in a Unit of Work. It owns rules no single Aggregate can decide (e.g. uniqueness across all users), but not rules an Aggregate can enforce itself.

**Repository**:
The interface, defined by a Module's Application Layer, through which a Service loads and saves Aggregates. The DB Layer provides the real implementation. The seam that keeps the database out of the Application and Domain Layers.

**Unit of Work**:
One database transaction a Service opens for a use case: every Aggregate it saves, and every Event it publishes into it, commits together or not at all.

**Clock**:
Where the application reads the time. Injected, so a test can fix it and move it.

## Between Modules

**Module API**:
The small, pure surface a Module offers to other Modules in-process, in its own `<module>-api` crate: capability traits, Published Records, Topics and a description of its Seed, with ids crossing as plain values. The only way one Module may depend on another. Distinct from the Web Layer, a Module's HTTP surface for clients.
_Avoid_: Interface crate, contracts

**Published View**:
A versioned, read-only view of a Module's data (e.g. `users_api.user_v1`) that other Modules' SQL may read, in a schema named for the Module API: a data product the Module publishes for querying. It is to SQL what the Module API is to Rust. It only gains columns; a breaking change is a new version beside the old. No Module reads them any more: a Module keeps a Projection instead.

**Published Record**:
The plain Rust row of a Published View (e.g. `users_api::v1::User` for `users_api.user_v1`), in the providing Module's API. It describes the Module's Seed data to other Modules and is what their tests assert against. It never stands in for the Aggregate, and a capability trait never returns it.

**Read Module**:
A Module that owns no source data and enforces no business rules: it answers queries no one Module owns (notes with their authors' names) from its own Projections of other Modules' Aggregate Roots, returning Read Models. It has Web, Application and DB Layers but no Domain Layer.

**Projection**:
A Module's own copy of Aggregate Roots, kept in its own tables from their Topics by Subscriptions starting from `earliest`: usually another Module's roots, to list or search by their columns, or its own in a shape its Aggregate's tables can't serve. Any Module may keep one; a Read Module is one that keeps nothing else. Common code applies each Event only when it is newer than the copy and keeps a tombstone for a removed root; the Module writes only its rows. It **catches up** once every Event its Topics held when its Subscriptions were registered, relayed by then or not, is delivered and handled. A rebuilt copy is a new **Generation** (new tables, new queues), served once it catches up; until any has, a read served from it answers that it is unavailable.
_Avoid_: Cache, replica

**Read Model**:
A shape assembled for reading, such as a note with its author's name, rather than an Aggregate. It is never saved and protects no invariants.

**Enrichment**:
Adding another Module's columns to a page a Module has already read from its own tables, with one batch call to the provider's Module API for that page's ids. For showing only: sorting, filtering, searching or paging by those columns needs a Projection.

## Events

**Event**:
Something that happened to an Aggregate, announced for Modules to react to (e.g. a user registered). A fact, named in the past tense, never a request for something to be done. Its type is published in the Module API as one of its Topic's, and it carries an Event Payload. Every change to an Aggregate publishes one, recorded in the Outbox in the same Unit of Work as the change. The publisher doesn't know who listens.
_Avoid_: Message, command, notification

**Topic**:
Every Event that happens to one kind of Aggregate Root (e.g. `users.user.v1`), keyed by the root's id. Declared in the Module API with an exhaustive list of its Events and a **Retention**, how long the Outbox keeps its history, and versioned as a whole.

**Event Publisher**:
The interface, in common code, through which a Service publishes an Event into its Unit of Work, beside the Repository it saves through. The DB Layer's implementation writes it to the Outbox.

**Event Payload**:
What an Event carries: the **entity**, the Aggregate Root's whole state as it now is but its audit fields, so a Subscriber need not call back (event-carried state transfer); and a **context** saying what happened where the state can't (e.g. the id of the Child Entity removed).

**Event Envelope**:
The CloudEvents wrapper every Event is stored and delivered in: its id, source Module, Topic, type, key (the root's id), the root's version and time, around its Event Payload.

**Subscriber**:
A Module's reaction to an Event, or to every Event of a Topic, another Module's or its own (e.g. archiving a deleted user's notes, welcoming a registered user). It changes only its own Module's data, and may run more than once. Each is a **Subscription**: its own queue, registered when migrating, which **starts** from `latest` (only Events relayed after it was registered) or `earliest` (also given every Event its Topic's Outbox held when it was registered, copied in by the Worker a chunk at a time).
_Avoid_: Listener, consumer, handler (for the concept)

**Outbox**:
Where a Unit of Work records its Events, so they are delivered if and only if it commits. Shared by every Module and owned by none. A **compacted** log: the Worker deletes a relayed Event once it is older than its Topic's Retention and a later one of its key exists, so every key's latest Event stays forever.

**Worker**:
The process that delivers Events to their Subscribers, each delivery in its own Unit of Work, in order per key, retrying a failure and setting it aside as a **Dead Letter** once its attempts run out.

**Bus**:
The broadcast between the app's instances (ADR 0039), so a stream held by one hears what happened on another. Best effort, unlike the Outbox: a message sent while an instance can't hear it is lost to it, and it is told only of a **Gap**, its cue to recheck the truth the database holds. Carries named **Channels**; a **Hub** fans one instance's subscription out to the streams it holds.
_Avoid_: Event bus, message queue (it delivers no Events and queues nothing)

## HTTP

**Spec**:
The OpenAPI description of the backend's HTTP surface, derived from the code (the code is the source of truth). The contract the frontend is generated against.
_Avoid_: Schema, swagger

**Generated Client**:
The TypeScript produced from the Spec: the types and data-access layer the frontend calls the backend through. A build output, never edited by hand.

**Idempotency Key**:
A client-chosen value sent as the `Idempotency-Key` header on an unsafe request (POST, PUT, PATCH, DELETE), so that a retry with the same key and the same request runs at most once and gets the first successful answer replayed. Scoped to its caller (ADR 0032).
_Avoid_: Request id (which only correlates logs and responses)

**Tool**:
An operation of the Spec as an AI agent calls it over MCP, at `/mcp`: named by the operation's id, described by its summary, and answered by sending the operation's own request in-process (ADR 0034). Every operation is one unless deliberately kept REST-only.
_Avoid_: Endpoint, command (for a tool)

## Authorization

**Principal**:
Who is acting: a user, with an id and the roles their token gives them, or `System`, the worker's and seeds' caller, acting on facts already decided, which policies don't apply to. Built on every guarded request from the token's claims (ADR 0040).
_Avoid_: User (for the caller), identity, subject

**Policy**:
A Module's YAML in its core's `policies/`, embedded in the binary and checked against its Rust types when it loads: a resource's rules (what is denied or allowed, to which roles, on which condition), its Scopes, and its default Grants (ADR 0040).
_Avoid_: Permission file, ACL

**Role**:
A record in the `access` Module that Grants are made to: a name, which is its id and never changes, and a description. Admins create and delete them (`list_roles`, `create_role`, `delete_role`); `admin` and `user` are built in and can't be deleted, and one a Grant still names can't be deleted until the Grant is revoked. `migrate` creates a role a Default Grant names when it first applies that Grant. A Principal still takes its roles from the token.
_Avoid_: Group, permission set

**Grant**:
A Role given an action on a resource, within a Scope or on any row. It only allows. Stored by the `access` Module, where admins make and revoke them at runtime; a Grant its Module's policies no longer have (a later release removed its action or scope) is stale, listed as such and skipped by the rule set.
_Avoid_: Permission, entitlement

**Scope**:
A named condition on a resource's rows (`author`: the note's author is the Principal), which a Grant can pair with any role. A derived role whose `parentRoles` is `["*"]`.
_Avoid_: Filter (for its name), row rule

**Default Grant**:
A Grant a Module's `policies/default_grants.yaml` declares, which `migrate` applies once, as `System`, recording its key (Module, role, resource, action, Scope) in `access`, and creating its Role first if none has that name. A revoked one stays revoked, and its Role, once an admin deletes it, stays deleted; one added in a later release is granted by that release's `migrate`.
_Avoid_: Seed (seeds don't run in production), built-in permission

**Revision**:
The counter `access` bumps in the unit of work of every change to the Grants or Roles. Each server instance polls it every 5 seconds and, when it has moved, rebuilds its rule set from every Module's policies and every Grant, and swaps it in whole.
_Avoid_: Version (that is a root's), generation

**Catalog**:
What admins grant from (`get_access_catalog`): every Module's Gate and resources, each action with the operations performing it and whether anything in force could allow it, and each resource's Scopes. Built from the loaded policies and `OPERATIONS`, never stored.
_Avoid_: Permission list, schema

**Gate**:
A Module's resource `module:<name>` and its one action, `use`, granted on roles only: what a Principal must pass before any of the Module's guarded operations, through its `Caller`.
_Avoid_: Middleware, guard (for this)

**Plan**:
What the rule set answers for a Principal's action: `Allowed`, `Denied`, or `Where(filter)`, the conditions left once the Principal's roles are known, checked against a loaded row or pushed into a list's query. A list's repository takes it as `Rows` (`All` or `Where(filter)`), never `Denied`: the service has refused that already.
_Avoid_: Decision (for a `Where`), verdict

## Testing

**Integration Test**:
The default test style: exercises a Module through its `Endpoints` and every real Layer against real dependencies run locally (the development database server, a fresh database per test), starting from the Module's Seed, or against Fakes where a dependency can't run locally. Contrast with an inline unit test, reserved for pure logic with many possible states.

**Driver**:
The only code in a Module's tests that knows HTTP: it exposes domain verbs and returns responses or refusals, so tests speak the domain.

**Test Template**:
A database a test binary migrates and seeds once per run, from which each of its tests gets a fresh copy (ADR 0031).
_Avoid_: Fixture database, template (alone, which means the repository)

**Seed**:
A Module's deterministic, well-known dataset: named entities with stable ids and fixed timestamps, built through the domain's own constructors and saved through the real Repository. Tests start from a Seed and refer to its entities by name. A Module seeds only data it owns; what other Modules may know of its Seed is described in its Module API, and a Seed names another Module's Seed data only through that.
_Avoid_: Fixtures

**Fake**:
A hand-written, in-memory implementation of an interface, used in tests in place of a real external dependency that can't run locally (e.g. a mail server). The database always can, so a Repository is never faked. A Fake is behaviour, not a recorded script: distinct from a mock.

**Journey**:
One test that walks a path across Modules on the composed Server (every registered Module, the real Worker, real auth, nothing stubbed), asserting at each step, to check that the Modules fit together. Rules are tested by a Module's own Integration Tests; a path within one Module is one of those, not a Journey.
