# Tests are integration-first, through a Module's Endpoints, against real Postgres

A Module is tested from outside: through its `Endpoints`, through every real Layer, against the docker-compose Postgres that local development uses and CI starts too. Rules live on Aggregates, but an Aggregate's rule is still tested through HTTP, with one exception: inline unit tests cover pure Domain logic whose state space is combinatorial (`proptest`), and `app-persistence`'s `Handlers`, the shared routing every Module's Subscribers rely on, against a hand-written `TxBackend`.

Each Module has one test target, named after it, with a driver: the only code that knows routes, methods, status codes and headers. It exposes domain verbs and returns the response or a `Refusal`, mapped from the error body's `code` rather than the status, so a broken rule (`Invalid`) is told apart from an unparseable request (`Malformed`). Tests read as behaviours and compare against Seeds (ADR 0028). Background work runs only when a test runs it, and time moves only when a test moves it (ADR 0020), so tests stay deterministic.

Each test originally got a fresh database from `#[sqlx::test(migrations = false)]`, migrated and seeded in the test. [ADR 0031](0031-tests-copy-a-per-binary-template-database.md) replaces that with a copy of a per-binary template; the rest stands.

## Considered options

- **A testcontainers Postgres per test** (the first choice). About 3.5 times slower than a `CREATE DATABASE` on one running server.
- **A shared testcontainers instance feeding `#[sqlx::test]`.** sqlx reads `DATABASE_URL` before the test body, so the container would have to start before `main` in every test binary, and statically held containers are never removed.
- **Service tests.** Every use case is reachable through the `Endpoints` or the worker verbs, and a fresh database costs the same either way. Add one only when a use case has neither, or two outcomes are indistinguishable over HTTP.
- **In-memory fakes of Postgres.** The database can always run locally, so a Repository is never faked. A Fake is only for an external dependency that can't (`users`' `FakeMailer`).

## Consequences

- Tests need compose up: `make test` depends on it, but a bare `cargo test` without it hangs about 30 seconds per binary, then fails `PoolTimedOut`.
- Tests point `DATABASE_URL` at the `postgres` database, never `app`, and keep `SQLX_OFFLINE=true` so `query!` compiles against `.sqlx` rather than that database.
- A Module's tests stand in for whatever is across its boundary (a stubbed Module API trait, a hand-published provider Event), so whether Modules fit together is the Journeys' to check (ADR 0029).
- Frontend tests follow the same rule: real components and the real Generated Client, mocked only at the network boundary with MSW, never with module or import mocks.
