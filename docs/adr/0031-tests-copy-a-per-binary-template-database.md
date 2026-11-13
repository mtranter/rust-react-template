# Tests get a database copied from a per-binary template

Supersedes the `#[sqlx::test]` part of [ADR 0004](0004-integration-first-tests-through-the-endpoints.md); the rest of it stands.

`#[sqlx::test]` migrates and seeds every test's database from empty, and gives all of a test binary's tests one pool of 20 connections. A test that holds several connections at once (a Worker beside a request) waits on the others, and under load times out. Every test also pays for every migration and Seed.

So each test binary declares a template (`app_test_db::Template`, in its driver: a short name and a build function that migrates and seeds), and each test is `#[app_test_db::test(crate::driver::TEMPLATE)] async fn name(pool: PgPool)`. The first test of a run builds the template database (`_test_template_<name>`) once, dropping any left from an earlier run so a changed migration or Seed is never stale. Each test then gets `CREATE DATABASE … TEMPLATE` of it, named from a hash of its test path, and a pool of its own. A passing test's database is dropped; a failing one's is kept to inspect, until that test runs again.

## Considered options

- **`#[sqlx::test]` with a larger pool.** Every test still migrates and seeds from empty, and the pool is still shared.
- **One database for all tests, isolated by transactions.** The Worker and the request run in separate transactions and must see each other's commits.

## Consequences

- Copying a migrated, seeded database is much cheaper than building one; a database can't be copied while anyone is connected to it, so the template's pool is closed once built.
- Template names must be unique across test binaries, and two runs against one server at once (two worktrees, a watcher beside `make test`) still collide.
- Failed or renamed tests leave databases behind: `make test-db-clean` drops them.
- `DATABASE_URL` names the server and the database to manage the others from (`postgres`).
