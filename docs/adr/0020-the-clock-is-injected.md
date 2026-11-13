# The Clock is injected, never ambient

Services and the Worker read the time from `app_domain::Clock`, held as `Arc<dyn Clock>`. The Server builds one `Context` with a `SystemClock` and passes it to every Module; test drivers build theirs with a `FixedClock`, set after the Seeds' times, which they `advance()`. The Domain never reads the time: constructors and actions take `now` (ADR 0007). The Clock works at `timestamptz` precision, so a time read back from Postgres equals the one written.

## Considered options

- **An ambient global or task-local clock.** Work spawned off a request would silently read the real time in tests.

## Consequences

- Tests control time and backoff: time moves only when a test moves it (`later(by)`).
