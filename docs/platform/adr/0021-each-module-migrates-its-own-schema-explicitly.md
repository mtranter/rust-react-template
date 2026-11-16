# Each Module migrates its own schema, explicitly, never on startup

Each Module writes to its own Postgres schema (`users`, `notes`), plus `<schema>_api` for the Published Views it offers (ADR 0010), and owns its `migrations/`. The `migrate` bin applies every Module's, in registration order (ADR 0019), after `app-db`'s own `messaging` schema, each with its own `_sqlx_migrations` table in its schema; then it registers Subscriptions (ADR 0014). Queries are `sqlx` macros (`query!`), checked at compile time against a checked-in `.sqlx` cache with `SQLX_OFFLINE=true` as the default, so building and CI need no live database.

Migrations run explicitly (`make migrate`), never silently on server startup, so production controls when they run. Seeding non-production environments follows the same rule: `server`'s `seed` bin runs every Module's `seed::standard`, exists only with the `seed` feature, and refuses `APP_ENV=prod`. The image ships `migrate` (and `seed`, when built with it) beside `server`, and a deployment runs them as one-off pre-start tasks inside its network (an init container, a pre-deploy job), never from CI.

## Considered options

- **Migrating on startup.** Every replica would race to migrate, and a rollout couldn't separate migrating from serving.
- **One migrator for the whole database.** Schema-per-Module keeps a Module's tables, and its migration history, its own, so it can be lifted out (ADR 0001).

## Consequences

- `sqlx::migrate!` embeds migrations at compile time, so each crate with `migrations/` has a `build.rs` that reruns on changes; without it Cargo can reuse a build that lacks a new migration.
- `#[sqlx::test]`'s own migrator would apply `migrations/` into `public` with one `_sqlx_migrations` table, bypassing schema-per-Module, so tests never use it (ADR 0031).
- Changing a `query!` needs `make prepare` against a migrated database.
- The Worker refuses to start until `migrate` has registered its Subscriptions.
