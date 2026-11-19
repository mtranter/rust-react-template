# Learnings: 0056 `worker --drain`

- DB tests need `SQLX_OFFLINE=true DATABASE_URL=postgres://app:app@localhost:5432/postgres` (database `postgres`, not `app`); the worker's delivery tests are `cargo test -p app-db --test delivery <filter>`.
- `crates/server` sets `autotests = false`: a new file in `crates/server/tests/` needs its own `[[test]]` entry in `crates/server/Cargo.toml`. Integration tests there can run the real binaries via `env!("CARGO_BIN_EXE_worker")`.
- `app_db::connect` retries a refused connection until sqlx's 30 s acquire timeout. To test "database unreachable" quickly, use a host under `.invalid` (fails DNS at once).
- The delivery tests' `Recorder` errors with `AppError::conflict("not now")`, which is stored in `last_error` as `"conflict: not now"`.
- `messaging.hangs(key, v)` makes a handler never return: handy for budget/timeout tests.
- The binaries hand-roll argument parsing (no clap in the workspace); `worker.rs` has a small `mode(args)` with inline unit tests.
