# Endpoints are unit structs, so the Spec is generated without a database

The code is the source of truth for the Spec. Every Module's `XEndpoints` is a unit struct with no fields, and each handler takes its Service as request data (`Data<&XService>`), which the Module's provider in `di.rs` builds from the shared `Context` and the Server injects. `OpenApiService::spec()` returns the Spec synchronously, with no server and no async runtime, so `dump-spec` builds the same service as the Server (one `build_service()`, same title, version and server URL) from `Endpoints::default()` and prints it, needing no database and no pool. Generating the Spec, and so the Generated Client (ADR 0023), is a pure sub-second step: the fast inner loop depends on it.

## Considered options

- **Services held as state on the `Endpoints` struct.** Building the Spec would need a pool, so a database, to construct them.

## Consequences

- Handlers never see the pool, so the Web Layer needs no sqlx. Testability comes from the Repository trait (ADR 0007), not from state on `Endpoints`.
- At runtime the Server also serves the Spec at `/api/spec.json` and Swagger UI at `/docs`.
