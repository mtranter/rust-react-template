---
id: 0020
name: A worker refuses to start when its clock is far from the database's, since leases and due times are judged by worker clocks
status: done
priority: P2
size: S
blocked_by: []
blocks: []
source: evaluation of the pub-sub mechanism 2026-11-15 (a note, not an observed bug); decision by Mark the same day (fail to start, loudly)
---

# A worker refuses to start when its clock is far from the database's, since leases and due times are judged by worker clocks

## Description

Every time the worker compares is taken from its own injected `Clock` (ADR 0020), never from the database:

- a claim treats a job as due when `run_at <= <this worker's now>`, and leases it until `<this worker's now> + 5 minutes`;
- a failure backs off to `<this worker's now> + backoff`;
- compaction deletes events older than `<this worker's now> - retention`.

With several workers, these are different clocks. Ordinary NTP skew (milliseconds) is harmless. Skew that approaches the lease is not: a worker whose clock runs more than five minutes ahead of another's sees that worker's freshly leased jobs as already due and runs them concurrently, and one running far ahead compacts events earlier than their topic's retention allows, which can't be undone. A worker running behind only delays work.

This is a latent operational hazard, not a bug seen in practice, and ticket 0013 removes clocks from the one place where milliseconds mattered (readiness).

**Decision (2026-11-15): a worker whose clock is too far from the database's refuses to start.** A worker that won't start is loud and loses nothing: events wait in the outbox and other workers carry on. A worker that runs with a bad clock double-runs jobs and deletes history. If the database's clock is the wrong one, every worker refuses and delivery stops until it is fixed; that is accepted, for the same reason.

## Evidence (as of 2026-11-15)

- `crates/common/db/src/worker.rs`: `run_next` (`now`, `now + LEASE`), `fail` (`now + backoff(..)`), `relay` (`run_at` and `created_at` of new jobs), `compact` (`$1::timestamptz - t.retention`), all from `self.clock.now()`.
- `crates/server/src/bin/worker.rs`: builds the worker with `SystemClock` and calls `worker.run(..)`, which calls `check_subscriptions()` first and refuses to start if it fails.
- `crates/common/db/tests/delivery/polling.rs`: three tests call `Worker::run` with a `FixedClock` at seed time, years from the database's clock.

## Proposed approach

Keep the injected clock. Moving the comparisons to the database's `now()` would fix skew but remove the tests' control of time (`Messaging::later`, `FixedClock`), which ADR 0020 exists to give. Guard the assumption instead.

### 1. `Worker::check_clock`

A public method beside `check_subscriptions`, in `crates/common/db/src/worker.rs`. It reads the database's time with `SELECT clock_timestamp()` (not `now()`, which is the transaction's start) and compares it with `self.clock.now()`:

- more than **1 minute** apart, either direction: return an error naming both times and the difference, in the style of `Unregistered` / `OtherTopic` (a `thiserror` struct wrapped in `AppError::internal`);
- more than **5 seconds** apart: `tracing::warn!` with the difference, and return `Ok`;
- otherwise silent.

Name both thresholds as constants beside `LEASE`, with a comment giving the derivation: two workers each within a minute of the database are within two minutes of each other, well inside the 5-minute lease. The round trip of the query adds to the measured difference, which is why the warning threshold is seconds rather than milliseconds.

### 2. The `worker` binary calls it, not `Worker::run`

`crates/server/src/bin/worker.rs` calls `worker.check_clock().await?` before `worker.run(..)`, so the process exits non-zero with the error. Log it at error first so it shows in structured logs as well as on stderr.

Do **not** put the check inside `Worker::run`: the polling tests run it with a fixed clock far from the database's and would all refuse. A test that drives time itself is exactly the caller that must be able to skip the check.

### 3. Re-check before each compaction

A start-up check misses a clock that drifts later, and compaction is the irreversible operation. In `Worker::compact_if_due`, once a compaction is due, apply the same 1-minute rule: if the skew is over it, skip that compaction, log at error, and leave the worker running (it tries again at the next period). Don't stop a running worker mid-flight for this.

The same problem as in step 2 applies: tests call `compact_if_due` and `run` with a fixed clock (`polling.rs`, `compaction.rs`). So this re-check must be off unless asked for. Add a builder in the style of `compacting_every` (for example `Worker::checking_its_clock()`), set by the `worker` binary only, and have `compact_if_due` consult it. `check_clock` itself stays callable regardless.

### 4. Docs

- `LEASE`'s doc comment: workers' clocks must agree to well within it, and what enforces that.
- `crates/AGENTS.md`, "Running and deploying": a worker refuses to start when its clock is more than a minute from the database's, and skips compaction while it is.
- ADR 0020 (the clock is injected): a short note that the worker checks its injected clock against the database's at start-up, and why the comparisons themselves stay on the injected clock.

## Acceptance criteria

- [x] A test in `crates/common/db/tests/delivery/`: `check_clock` fails for a worker whose `FixedClock` is more than a minute from the database's (seed time is), and the error names the difference.
- [x] A test: `check_clock` passes for a worker on a clock within the threshold (a `FixedClock` set from `Utc::now()` in the test, or `SystemClock`; this test file may need the `disallowed_types` allowance its `main.rs` already has).
- [x] A test: a worker built with the re-check on and a clock far from the database's does not compact when a compaction is due (an event eligible for compaction is still in the outbox afterwards), and one built without it compacts as today.
- [x] The `worker` binary calls `check_clock` before `run` and enables the re-check; `Worker::run` itself does not check the clock, and every existing test in `polling.rs` and `compaction.rs` passes unchanged.
- [x] The docs above are updated.
- [x] `.sqlx` regenerated; `make check` passes.

## Resolution

Done as proposed. `Worker::check_clock` compares the injected clock with `SELECT clock_timestamp()`: over `CLOCK_REFUSED` (1 minute) it fails with `ClockSkew`, naming both times and how far apart and on which side; over `CLOCK_WARNED` (5 seconds) it warns. The `worker` binary calls it before `run`, logging the refusal at error, and builds the worker `checking_its_clock()`, so `compact_if_due` refuses a due compaction while the check fails; `run` logs that at error and the next period tries again. `Worker::run` doesn't check, and tests stay on their fixed clocks.

Decided in discussion the same day: both sides are instants (`DateTime<Utc>`, `timestamptz`), so host and database time zone settings can't trip the check. A database whose own clock is wrong does stop every worker although the workers would agree; that stays accepted, with no override, since managed databases keep good time and it is easy to add one if needed.
