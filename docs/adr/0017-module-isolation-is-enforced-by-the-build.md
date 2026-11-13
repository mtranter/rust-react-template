# Module isolation is enforced by the build, in layers

Isolation the docs promise but nothing checks erodes with the first convenient import. So each rule has a check in `make check` and CI:

1. **The Cargo graph** (primary). A crate that doesn't list a sibling can't import it, which carries most of the isolation for free and is stronger than a test. `make check-deps` reads `cargo metadata` and fails if a Module core's normal or build dependencies include another core (dev-dependencies may: a Read Module's tests run its providers), if an `-api` crate depends on anything but a pure common crate or another `-api`, or if a write Module declares `reads`.
2. **`cargo-deny` bans.** `sqlx` and `poem` are denied workspace-wide, with `wrappers` allow-lists of who may depend on them directly: `app-db` for sqlx, `app-web` and `server` for poem, and every Module core, Read Modules included, for both. An `-api` crate never, so every pure crate stays framework-free. Bans target third-party crates; the Cargo graph handles internal edges.
3. **A per-crate `clippy.toml`** bans naming `sqlx::PgPool`, `sqlx::Pool` and `sqlx::Postgres`, under `-D warnings`. Only `db/mod.rs`, `di.rs`, `seed.rs` and test targets opt out (they wire or receive the pool), never `application/` or `domain/`. A tripwire, not a boundary: other sqlx types, poem and a local `#[allow]` get past it.
4. **`make check-sql`** refuses cross-schema SQL that isn't a declared Published View (ADR 0010).
5. **`pnpm run lint:imports`** fails a frontend feature importing another, or code outside a feature importing anything but its `index.ts` (ADR 0024). A script, because Biome's `noRestrictedImports` matches exact module names only.

## Considered options

- **`rust_arkitect`** for layering assertions inside a crate. Crate-per-Module (ADR 0001) makes it largely unnecessary.
- **Per-Module Postgres roles**, each granted `SELECT` only on other Modules' `_api` schemas: the database-enforced version of `check-sql`. Not built.

## Consequences

- A new Module goes into both `deny.toml` lists; `make check-modules` fails if it doesn't (ADR 0019).
- `check-sql` is a grep, not a parser, and clippy can be silenced locally: they catch mistakes, not intent.
