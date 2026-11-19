---
id: 0056
name: "`worker --drain [--max-seconds N]` runs everything due by its clock, then exits"
status: done
spec: docs/platform/specs/0004-modules-schedule-deferred-and-recurring-work-through-the-existing-queue.md
blocked_by: []
blocks: []
---

# `worker --drain [--max-seconds N]` runs everything due by its clock, then exits

`worker --drain` lets any external timer (cron, a container scheduler) drive the worker, with no state outside Postgres (Spec 0004). It runs the same pass as the poll loop (`run_until_idle`, which relays, backfills, turns Schedule slots into jobs once they exist, and runs jobs) until nothing is due by its clock, or until the `--max-seconds` budget is spent, then exits `0`. Job failures are retried and dead-lettered as usual, and don't affect the exit code. It exits non-zero only when it can't work at all: the database is unreachable, or `check_clock` refuses its clock. `worker` without `--drain` is unchanged.

## Acceptance criteria
- [ ] `crates/server/src/bin/worker.rs` accepts `--drain` and `--max-seconds N`. `--max-seconds` without `--drain` is refused with a usage error.
- [ ] A test against Postgres with the test clock: the drain returns once nothing is due, including leaving a job whose `run_at` is in the future.
- [ ] A test: with more due work than the budget allows (or a handler that outlasts it), the drain stops once the budget is spent and returns success.
- [ ] A failing handler during the drain is retried and dead-lettered as usual, and the drain still returns success.
- [ ] An unreachable database or a refused clock exits non-zero, with the cause logged.
- [ ] `worker` without flags behaves exactly as before.
- [ ] `crates/AGENTS.md` (the "one image, three processes" bullet) documents `--drain` and `--max-seconds`.
- [ ] `make check-changed` passes.
