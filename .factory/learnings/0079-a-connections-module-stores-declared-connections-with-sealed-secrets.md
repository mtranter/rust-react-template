# Learnings: 0079, a connections module with sealed secrets

- **The dev database may be another worktree's.** Postgres on :5432 can belong to another
  worktree's compose project, so `docker compose exec postgres` (used by
  `scripts/migrate-schema.sh` and `migrate-unregistered.sh`, so by `make new-module`) fails
  with "service postgres is not running". Apply a schema by hand instead:
  `psql "$DATABASE_URL" -c 'CREATE SCHEMA IF NOT EXISTS x'` then
  `cargo bin sqlx migrate run --source <core>/migrations --database-url "$DATABASE_URL?options=-csearch_path%3Dx"`.
- **Refreshing `.sqlx` for a new module, in order:** `make migrate` *before* the server depends
  on the new crate (its `query!`s can't compile offline yet), then apply the new module's and the
  unregistered modules' (template, reference) schemas, then
  `SQLX_OFFLINE=false DATABASE_URL=postgres://app:app@localhost:5432/app cargo bin sqlx prepare --workspace -- --all-targets`.
  A failed `prepare` deletes `.sqlx` files: `git checkout .sqlx` and rerun. Tests run with
  `SQLX_OFFLINE=true` against the `postgres` database; `SQLX_OFFLINE=false` there fails.
- **The linker can be OOM-killed** (`ld terminated with signal 9`) linking the server's bins and
  test binaries in parallel: `CARGO_BUILD_JOBS=2` or `4` gets through.
- **A platform module is registered by hand** (`make new-module` only writes
  `crates/capabilities/`): copy `crates/module-template`, and add it to `modules.rs`, the
  server's `Cargo.toml`, the root `Cargo.toml`, both `deny.toml` lists, the `Reference`
  composition in `crates/reference/journeys/src/lib.rs`, `scripts/init.sh` (its `modules.rs`
  and `TOOLS`), `scripts/check-init.sh` (`platform_tools`, `platform_ops`) and the composed
  app's `TOOLS`. A platform Capability with no App goes in `appless` in `scripts/check-modules.sh`.
- **Something a module declares for another to record at `migrate`** goes through
  `Module::declaring(T)` / `Module::declared::<T>()` (type-erased, so `app-module` depends on no
  module), collected by a `server::` function `migrate` calls. Reuse it rather than adding a
  typed field to `Module`.
- **The platform's journeys live in `crates/reference/journeys/`** (there is no
  `crates/server/tests/journeys/` in the template). With no reference module declaring a
  Connection yet, the journey records one itself (`App::declares`) as `migrate` would.
- `#[non_exhaustive]` on an `-api` struct forbids struct-update syntax in other crates (tests
  included): mutate its pub fields instead.
- `ring` (already in the tree) gives AES-256-GCM, SHA-256 and a system RNG: no `aes-gcm` crate is
  needed.
