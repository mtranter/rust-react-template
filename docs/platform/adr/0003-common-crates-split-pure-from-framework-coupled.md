# Common crates are split into pure and framework-coupled

There is no single `common` crate. Shared code is several focused crates, split on whether they carry a web or database dependency. The pure ones (`app-domain`, `app-error`, `app-events`, `app-persistence`, and the dev-only `app-test-support`) never depend on `poem` or `sqlx`, so the Domain and Application Layers and every Module API crate can use them. The framework-coupled ones are `app-web` (poem), `app-db` (sqlx) and `app-module` (both, for `di.rs` and the Server). Config and tracing live in `crates/server`, not in common.

A god-`common` pulling in poem and sqlx would make the Domain depend on both transitively, break the framework-free Domain and the `cargo-deny` bans (ADR 0017), and recompile everything on any change.

## Considered options

- **One `common` crate.** Rejected for the reasons above.

## Consequences

- More crates to wire.
- `app-error` (pure) and `app-web` (its HTTP mapping) are deliberately separate: Services return `AppError` with no knowledge of poem, and `app-web` alone translates it to HTTP (ADR 0022). Domain actions return the Module's own error enum, which the Application Layer maps onto `AppError`.
- Folders under `crates/common/` are short (`web`, `db`), but packages are prefixed `app-*`, to avoid crates.io collisions and read clearly at the `use` site.
