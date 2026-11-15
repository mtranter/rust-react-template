---
id: 0016
name: A job whose worker dies on every attempt is dead-lettered instead of being retried forever
status: done
priority: P2
size: S
blocked_by: []
blocks: []
source: evaluation of the pub-sub mechanism 2026-11-15 (found by reading; not reported by Copilot)
---

# A job whose worker dies on every attempt is dead-lettered instead of being retried forever

## Description

`Worker::run_next`'s doc says: "The claim counts as an attempt, so a job that kills its worker is dead-lettered too, eventually." That is not what the code does.

A job is dead-lettered only in `Worker::fail`, which runs only when the handler *returns* an error. If the handler takes the worker down instead (a panic, an out-of-memory kill, a deploy that stops the process mid-handler, a hang past the pod's liveness timeout), `fail` never runs. The claim has already pushed `run_at` forward by the lease, so five minutes later the job is due again and is claimed again. The claim increments `attempts` but never looks at it, so this repeats without end:

- the job is never dead-lettered, so nobody is told;
- every later version of its key in that queue waits behind it forever (the ordering guard holds them back while an earlier version is live);
- if the handler panics, each claim costs a worker restart.

## Evidence (as of 2026-11-15)

`crates/common/db/src/worker.rs`:

- `run_next`: the claim is `UPDATE messaging.job SET attempts = attempts + 1, run_at = $2 WHERE id = (…)`; the subquery filters on `dead_at IS NULL AND run_at <= $1` and the ordering guards, with no condition on `attempts`.
- `fail`: `let dead = job.attempts >= job.max_attempts;` is the only place `dead_at` is set by the worker.
- `MAX_ATTEMPTS` is 5 (`crates/common/persistence/src/messaging.rs`); `LEASE` is 5 minutes.

## Proposed approach

Dead-letter at the claim, with no extra query on the normal path.

The claim already returns `attempts` and `max_attempts`. A job claimed with `attempts <= max_attempts` runs as today (the claim that makes `attempts == max_attempts` is its last real attempt, and `fail` dead-letters it if it returns an error). A job claimed with `attempts > max_attempts` has had every attempt and the last one never finished: do **not** run its handler. Instead, in `run_next`:

- `UPDATE messaging.job SET dead_at = <now>, attempts = max_attempts, last_error = <reason> WHERE id = <id>`.
  - Reset `attempts` to `max_attempts` so `DeadLetter::attempts` still reads as the number of attempts made.
  - Keep any earlier error: for example `concat_ws(': ', 'no attempt finished: its worker stopped or outran the lease on the last one', last_error)`.
- Log at error level, as `fail` does for a dead letter (`queue`, `attempts`, the reason).
- Return `Ok(true)`, since work was done and the key's later versions may now be claimable.

Correct the doc comment on `run_next` to say how it happens.

### Interactions to keep in mind

- **A handler that is merely slow** (still running on another worker when its lease runs out for the last time) is also dead-lettered by this. That is acceptable: when the slow run finishes, `complete` deletes the job row, dead or not, and the dead letter disappears. Do not make `complete`'s delete conditional on `dead_at IS NULL`.
- **Rerunning a dead letter** is unchanged: clearing `dead_at` and `attempts` (as `DeadLetter`'s doc says) gives it a full set of attempts again.
- Ticket 0018 changes `complete`; the two don't conflict.

## Acceptance criteria

- [x] A test in `crates/common/db/tests/delivery/` (beside `a_dead_lettered_version_no_longer_holds_up_its_key` in `order.rs`): a job whose every attempt is abandoned mid-handler is dead-lettered after `MAX_ATTEMPTS` attempts, its handler is not run a further time, its `last_error` says why, and the next version of its key then runs.
  - Simulate a worker dying mid-handler faithfully if practical: a handler that never returns (`std::future::pending()`), with `worker.run_next()` wrapped in a short `tokio::time::timeout` so the future is dropped as a stopped worker's would be, then `messaging.later(..)` past the lease. This needs a "hangs" fault beside `Messaging::fails` in `support.rs`. If that proves awkward, set the job's `attempts` and `run_at` by SQL to the state such a job is in after its last lease expires.
- [x] A job that fails normally is still dead-lettered by `fail` after exactly `MAX_ATTEMPTS` attempts, with `DeadLetter::attempts == MAX_ATTEMPTS` in both cases; the existing dead-letter tests pass unchanged.
- [x] `run_next`'s doc comment matches the behaviour.
- [x] `.sqlx` regenerated if a `query!` changed; `make check` passes.

## Resolution

Done as proposed. `run_next` dead-letters a job claimed with `attempts > max_attempts` without running its handler (`Worker::abandon`): `dead_at` set, `attempts` reset to `max_attempts`, `last_error` prefixed with "no attempt finished: its worker stopped or outran the lease on the last one", logged at error level. `complete` is unchanged, so a slow run that finishes still deletes the dead letter. `run_next`'s doc comment, ADR 0013 and `crates/AGENTS.md` say so.

Tests in `order.rs`: `a_version_that_kills_its_worker_on_every_attempt_is_dead_lettered` and `a_version_that_fails_every_attempt_is_dead_lettered_after_its_last`. A worker dying mid-handler is simulated faithfully: `Messaging::hangs` makes a handler never return, and `Messaging::run_until_a_handler_hangs` drops the worker's future once the handler has begun, rather than after a fixed timeout, so the test doesn't depend on timing.
