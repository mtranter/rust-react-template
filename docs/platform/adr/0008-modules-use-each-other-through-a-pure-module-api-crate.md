# Modules use each other through a pure `<module>-api` crate

A Module others may use is two crates under `crates/modules/<module>/`: `core/` (package `<module>`) and `api/` (package `<module>-api`), its Module API. The `-api` crate holds only what consumers may see: capability traits (`users_api::UserDirectory`), Published Records, Topics and their Events (ADR 0012) and the Seed as others may know it (ADR 0028). It is pure: no poem, no sqlx. Consumers depend on `-api`, never on the core. The core implements the traits in its Application Layer and exposes a constructor from `di.rs` (`users::directory(ctx)`); the Server passes it to each consumer's `module(..)` (`notes::module(users::directory)`).

Ids cross as plain `Uuid`. The provider's typed id (`UserId`) stays in its Domain, and each consumer keeps its own (`notes::AuthorId`), converting in its Application Layer, so newtype protection holds on both sides, the seam is one `as_uuid()`, and a Domain names no other Module.

A Published Record is the Rust row of a Published View (ADR 0010), in a module named for the view's version: `users_api::v1::User` is a row of `users_api.user_v1`. It is always written with its version path, so it can't be mistaken for the Aggregate, and is `#[non_exhaustive]`, since consumers never build one, so a column the view gains breaks nobody. A breaking change is `v2::User` beside `v1::User`. A trait never returns one: traits have their own types, so a view gaining a column never changes a trait or a consumer's stub.

The Cargo graph keeps enforcing that no Module reaches into another's internals, while cross-module calls get a small, named, versionable surface. Contracts between crates also let two Modules use each other without a crate cycle.

## Considered options

- **Depending on the sibling's core crate.** Drags in its poem, sqlx and every internal.
- **`api/` and `private/` folders in one crate.** Nothing enforces them.
- **A consumer-owned port with every adapter in `server`.** Fine for one edge, but the glue piles up in `server`.

## Consequences

- A consumer's driver stubs the trait (the other Module isn't run); the provider tests its implementation against Postgres directly, since it has no HTTP entry point. Journeys (ADR 0029) check that the two agree.
- **Naming.** `-api` is the in-process capability surface, so a Module's HTTP Layer is `web/` and its operations `XEndpoints`.
