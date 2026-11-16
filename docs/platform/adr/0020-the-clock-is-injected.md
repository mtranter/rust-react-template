# The Clock is injected, never ambient

Services and the Worker read the time from `app_domain::Clock`, held as `Arc<dyn Clock>`. The Server builds one `Context` with a `SystemClock` and passes it to every Module; test drivers build theirs with a `FixedClock`, set after the Seeds' times, which they `advance()`. The Domain never reads the time: constructors and actions take `now` (ADR 0007). The Clock works at `timestamptz` precision, so a time read back from Postgres equals the one written.

## Considered options

- **An ambient global or task-local clock.** Work spawned off a request would silently read the real time in tests.

## Consequences

- Tests control time and backoff: time moves only when a test moves it (`later(by)`).
- Workers judge leases, due times and retention by their own clocks, so several workers' clocks must agree to well within the lease. The `worker` binary checks its clock against the database's at start-up and before each compaction, refusing past a minute (`Worker::check_clock`, ticket 0020). The comparisons stay on the injected clock: moving them to the database's `now()` would take time out of tests' hands. Both sides are instants, so time zone settings don't matter; a database whose clock is wrong stops every worker, which is accepted.
