---
id: 0055
name: "A Schedule removed from code is retired by migrate, with its pending run removed and no race with the worker"
status: done
spec: docs/platform/specs/0004-modules-schedule-deferred-and-recurring-work-through-the-existing-queue.md
blocked_by: [docs/platform/tickets/0054-a-module-declares-a-cron-schedule-that-the-worker.md]
blocks: []
---

# A Schedule removed from code is retired by migrate, with its pending run removed and no race with the worker

Completes the Schedule lifecycle in Spec 0004. When `migrate` finds a row in `messaging.schedule` that no registered Module declares any more, it retires the Schedule, as `messaging.retire` does for subscriptions. A retired Schedule creates no further slots, and its pending (not yet leased) run is removed. Retirement must not race a worker that is turning that Schedule's slot into a job or running it. Serialise it the way retiring subscriptions is serialised against the relay and backfill (tickets 0015, 0023), taking locks in the same order registration does.

## Acceptance criteria
- [ ] Running `migrate` without a previously registered Schedule marks the Schedule retired. Afterwards no worker pass creates a job for it (test).
- [ ] Retiring removes the retired Schedule's pending run from `messaging.job` (test).
- [ ] A run that is already leased when the Schedule is retired either finishes or is dead-lettered, never duplicated. A worker pass racing retirement never adds a job after it. The test is concurrent, modelled on ticket 0015's.
- [ ] Re-declaring a retired Schedule's name behaves as documented: either it is refused, or it is reactivated. Whichever applies, `crates/AGENTS.md` says so beside how to retire a Schedule.
- [ ] `make check-changed` passes.
