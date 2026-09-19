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
A horizontal concern *within* a module. There are exactly four:
- **API layer** — the module's externally-visible HTTP surface (its operations,
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
at the API layer from a request, passed through a Service to an Aggregate's
domain action.

### Modular Monolith
The overall architecture: many isolated modules compiled and deployed as a
single artifact. Isolation is real (enforced by the compiler and CI, see
`spec.md`), not merely conventional; the single artifact is a deployment choice,
not an isolation compromise.

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

### Seed
A module's deterministic, well-known dataset for tests: named entities with
stable ids and fixed timestamps, built through the domain's own constructors and
saved through the real Repository. Tests start from a Seed and refer to its
entities by name. A module's Seed never references another module's.

### Fake
A hand-written, in-memory implementation of an interface, used in tests in place
of a real external dependency when a real one cannot be run locally. The
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
