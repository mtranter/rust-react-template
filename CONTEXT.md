# Context

The canonical glossary for this template. Terms defined here have one meaning
across the codebase; if code or conversation drifts from these definitions,
the drift is the bug. This file is a glossary only — no implementation details,
no decisions (those live in `spec.md`).

## Glossary

### Module
A self-contained unit of the business domain (e.g. `users`, `notes`). Each
module is its own crate and owns everything it needs: its API surface, its
business logic, and its persistence — including its database schema and
migrations. A module is designed to be **liftable**: extractable into a
standalone microservice with minimal surgery, because it declares its own
dependencies and never reaches into another module's internals.

### Layer
A horizontal concern *within* a module. There are exactly four (a Read Module
has no Domain layer):
- **Web layer** — the module's externally-visible HTTP surface (its operations,
  request/response shapes). The only layer that knows about the web framework.
  Translates requests into Commands and hands them to a Service.
- **Application layer** — the module's use cases, expressed as Services. Defines
  the Repository interfaces. Knows nothing about HTTP or the database.
- **Domain layer** — the business rules, as Aggregates and Value Objects. Knows
  nothing about HTTP, the database, or Repositories. Depends only on pure crates.
- **DB layer** — persistence. The only layer that knows about SQL and the
  database driver. Implements the Repository interfaces the application layer
  defines.

### Service
An application-layer type that carries out a module's use cases. For each use
case it asks a Repository for an Aggregate, invokes a domain action on it with a
Command, and asks the Repository to save the result. It owns rules no single
Aggregate can decide (such as uniqueness across all users), but not business
rules an Aggregate can enforce itself.

### Aggregate
A domain object that owns its own consistency: its state can change only
through its domain actions, each of which either succeeds or returns a domain
error and leaves the Aggregate unchanged. It is what a Repository loads and
saves as a whole — never a row or data-transfer object.

### Value Object
A domain type defined by its value rather than an identity, valid by
construction (e.g. an email address). Holding one proves it passed validation.

### Command
A named intent to change the domain (e.g. register a user, edit a note). Built
at the Web layer from a request, passed through a Service to an Aggregate's
domain action.

### Modular Monolith
The overall architecture: many isolated modules compiled and deployed as a
single artifact. Isolation is real (enforced by the compiler and CI, see
`spec.md`), not merely conventional; the single artifact is a deployment choice,
not an isolation compromise.

### Module API
The small, pure surface a Module offers to *other modules* in-process:
capability traits, Published Records and a description of its Seed (ids cross
it as plain values), in its own `<module>-api` crate. The only way one Module
may depend on another. Distinct from the Web layer, which is
a Module's HTTP surface for clients.

### Published View
A versioned, read-only view of a Module's data (e.g. `users_api.user_v1`) that
other Modules' SQL may read, kept in a schema named after the Module's API: a
data product the Module publishes for querying. It is to SQL what the Module
API is to Rust: the Module chooses what it exposes and keeps its tables private.
A Published View only gains columns; a breaking change is a new version beside
the old. Only Read Modules read them.

### Published Record
The plain Rust row of a Published View (e.g. `users_api::v1::User` for
`users_api.user_v1`), in the providing Module's API. It describes the Module's
Seed data to other Modules and is what their tests assert against; it never
stands in for the Aggregate, and a capability trait never returns it.

### Read Module
A Module that owns no source data and enforces no business rules: it answers
queries that span Modules by joining their Published Views, returning Read
Models. It reads other Modules' data only through Published Views. It has a Web
layer, an Application layer and a DB layer, but no Domain layer. It may own data
derived from the views it reads (such as a search index), never data of its own.

### Read Model
A shape assembled for reading — a query result such as a note with its author's
name — rather than an Aggregate. It is never saved and protects no invariants.

### Common
Shared cross-cutting capability that is not owned by any one module. Split into
several focused crates rather than one bucket. A **pure common** crate carries
no web/database dependency and may be used by any layer, including the domain. A
**framework-coupled common** crate carries a web or database dependency and may
only be used by the layer that shares that concern.

### Repository
The interface, defined by a module's application layer, through which a Service
loads and saves Aggregates. The Service depends on the interface; the DB layer
provides the real implementation. This is the seam
that keeps the database out of the application and domain layers.

### Unit of Work
One database transaction a Service opens for a use case: every Aggregate it
saves, and every Event it publishes, commits together or not at all.

### Event
Something that happened to an Aggregate, announced for Modules to react to
(e.g. a user registered, a user was deleted). A fact, named in the past tense,
never a request for something to be done. Its type is published in the Module
API, versioned, and it carries an Event Payload. Every change to an Aggregate
publishes one. It travels in an Event Envelope and is recorded in the same Unit of Work as the change it announces, in the
Outbox. The publisher does not know who listens.

### Event Payload
What an Event carries: the entity, the Aggregate root's whole state as it now
is but its audit fields, so a Subscriber need not call back (event-carried
state transfer); and a context saying what happened where the state can't
(e.g. the id of the item removed from a list).

### Event Envelope
The CloudEvents-shaped wrapper every Event is stored and delivered in: its id,
source Module, type, subject (the Aggregate's id) and time, around its Event
Payload.

### Subscriber
A Module's reaction to an Event, another Module's or its own (e.g. archiving a
deleted user's notes, welcoming a registered user): it changes only its own
Module's data, and may run more than once.

### Outbox
Where a Unit of Work records its Events, so they are delivered if and only if it
commits. Shared by every Module and owned by none.

### Worker
The process that delivers Events to their Subscribers, each delivery in its own
Unit of Work, retrying a failure and setting it aside as a **Dead
Letter** once its attempts run out.

### Clock
Where the application reads the time. Injected, so a test can fix it and move it.

### Seed
A module's deterministic, well-known dataset for tests: named entities with
stable ids and fixed timestamps, built through the domain's own constructors and
saved through the real Repository. Tests start from a Seed and refer to its
entities by name. A Module seeds only data it owns; what other Modules may know
of its Seed is described in its Module API, and a Seed names another Module's
Seed data only through that.

### Fake
A hand-written, in-memory implementation of an interface, used in tests in place
of a real external dependency when a real one cannot be run locally (e.g. a mail
server). The
database always can, so a Repository is never faked. A Fake is behaviour, not a recorded script — distinct from a
mock.

### Spec
The OpenAPI description of the backend's HTTP surface, derived from the code
(the code is the source of truth). The Spec is the contract the frontend is
generated against.

### Generated Client
The TypeScript produced from the Spec — the types and data-access layer the
frontend uses to call the backend. It is a build output, never edited by hand.

### Server
The single deployable unit. It assembles every module's API into one service,
applies cross-cutting middleware, serves the Spec, and hosts the compiled
frontend. It contains no business logic of its own.

### Integration Test
The default test style: exercises a module through its real layers against real
external dependencies (run locally, e.g. the development database server, with
a fresh database per test), starting from the module's Seed, or against Fakes
where a real dependency cannot be run. Contrast with an inline unit test, which
is reserved for pure, side-effect-free logic with many possible states.
