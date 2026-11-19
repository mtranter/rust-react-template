---
status: draft
---

# Modules schedule deferred and recurring work through the existing queue

## Problem

ADR 0015 says a Module does work only in reaction to an event, and there is no scheduled work. Business workflows need it:

- **Deferred:** "remind in 3 days".
- **Recurring:** "close stale quotes nightly".
- **Timeouts:** "this approval expires in 48 hours unless someone acts", which is deferred work that can be cancelled.

Today a Module has no way to do any of these without inventing its own mechanism.

## Solution overview

Postgres stays the one source of truth for what is due. Both kinds of work become rows in `messaging.job`, which already has `run_at`, retries, dead letters and per-key ordering, run by the existing worker. ADR 0015 is superseded by a new ADR.

- **Deferred work:** a Unit of Work method writes a job for the Module's own handler, due at a given time and named by a key. The key lets it be rescheduled or cancelled.
- **Recurring work:** a Module declares schedules in code (a cron expression and a handler), as it declares topics and subscriptions. `migrate` registers them, and the worker turns each due slot into a job.
- **`worker --drain`:** relays, backfills, schedules and runs until nothing is due, then exits. Any external timer can then drive the worker (cron, a container scheduler, or later a cloud scheduler), with no state outside Postgres.

## Behaviour

### Deferred work

- A deferred job is a **command to the Module itself**, not an Event. It has its own type, with a stable name and a serde payload, and a handler registered like a subscriber's. This keeps ADR 0015's lesson: a command isn't dressed as an event.
- It's written in the same Unit of Work as the change that causes it, so it exists if and only if that change commits.
- It's named by a key within its type:
  - deferring again with the same key replaces the pending job's time and payload (reschedule);
  - cancelling by key removes a pending job;
  - cancelling a job that has already run, or never existed, is not an error.
- It runs as `Principal::System`, like reactions, with the same retries, backoff and dead letters as event jobs.
- It's due by the injected clock (ADR 0020), so tests advance time instead of sleeping.

### Recurring work

- A schedule has a name, a cron expression (five fields, UTC) and a handler.
- `migrate` registers it in a new `messaging.schedule` table:
  - a changed expression is updated, and its next due time recomputed;
  - a schedule removed from code is retired, and its pending run removed, as subscriptions are.
- The worker turns each due slot into one job. Several workers never create a slot twice.
- **After downtime:** missed slots run once, not once per missed slot.
- **No overlap:** a schedule never has two runs pending or running at once. A slot that falls due while the previous run is still going is skipped and logged.
- Handlers run as `System` and are told the slot they're running for.

### `worker --drain`

- `worker --drain [--max-seconds N]` runs until nothing is due by its clock, or until the time budget is spent, then exits `0`.
- Failures are retried and dead-lettered as usual, not reported by the exit code. It exits non-zero only when it can't do its work at all, for example the database is unreachable or its clock is refused.
- `worker` without `--drain` is unchanged.

### Reference code

`notes` (reference) shows one of each:

- **Recurring:** a nightly schedule purges notes archived more than 30 days ago.
- **Deferred:** an optional `archiveAt` on a note archives it at that time. It's keyed by the note, so changing it reschedules and clearing it cancels.

### Tests

Against Postgres, driven by the test clock:

- reschedule and cancel;
- a deferred job's change that rolls back leaves no job;
- missed slots run once;
- no overlapping runs;
- two workers create a slot once;
- a retired schedule's pending run is removed;
- `--drain` exits once idle, and when its budget is spent.

### Docs

- A new ADR supersedes ADR 0015.
- `crates/AGENTS.md` gains how to defer work and declare a schedule.
- `GLOSSARY.md` gains **Deferred Job** and **Schedule**.

## Out of scope

- Wiring a cloud timer (EventBridge Scheduler, Lambda), and a wake-up nudge after commit. Both belong to Lambda hosting (Spec 0010).
- Time zones and daylight saving: schedules are UTC.
- Schedules more frequent than once a minute.
- Schedules edited by admins at runtime: they're code.
- Showing schedules to operators. The operator surface (Spec 0005) adds that, whichever lands second.

## Open risks

- **Choice of cron crate.** It must fit the workspace's time crate: ticket 0030 plans to move off `chrono`.
- **Retiring a schedule** while its run is in flight must not race the worker. It should follow how retiring subscriptions avoids that (ticket 0015).
